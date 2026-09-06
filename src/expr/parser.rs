use super::lexer::{Token, TokenKind, tokenize};
use super::{BinaryOp, Expr, UnaryOp, invalid, list_expr};
use crate::Result;

/// Jet / Access expression. A single leading `=` (ControlSource, DefaultValue) is stripped.
pub fn parse_expr(input: &str) -> Result<Expr> {
    let s = strip_leading_eq(input.trim_start());
    if s.trim().is_empty() {
        return Err(invalid(0, "empty expression"));
    }
    parse_with(s, false)
}

/// ControlSource field name or bang/dot path, without a leading `=`.
pub fn parse_ident_path(input: &str) -> Result<Expr> {
    let s = input.trim();
    if s.is_empty() {
        return Err(invalid(0, "empty identifier"));
    }
    match parse_with(s, true) {
        Ok(e) if is_path(&e) => Ok(e),
        Ok(_) => Ok(Expr::Ident {
            name: s.to_string(),
            quoted: false,
        }),
        Err(_) if is_plain_field_name(s) => Ok(Expr::Ident {
            name: s.to_string(),
            quoted: false,
        }),
        Err(e) => Err(e),
    }
}

/// `=`-prefixed expression, otherwise an ident path (`Last Name`, `[Budget]`, `Form![ID]`).
pub fn parse_control_source(input: &str) -> Result<Expr> {
    let s = input.trim();
    if s.starts_with('=') {
        parse_expr(s)
    } else {
        parse_ident_path(s)
    }
}

fn strip_leading_eq(s: &str) -> &str {
    s.strip_prefix('=').map(str::trim_start).unwrap_or(s)
}

fn is_path(e: &Expr) -> bool {
    match e {
        Expr::Ident { .. } | Expr::Star => true,
        Expr::Qualified(parts) | Expr::Bang(parts) => parts.iter().all(is_path),
        _ => false,
    }
}

fn is_plain_field_name(s: &str) -> bool {
    !s.chars().any(|c| {
        matches!(
            c,
            '=' | '!'
                | '.'
                | '('
                | ')'
                | '['
                | ']'
                | '#'
                | '"'
                | '&'
                | '<'
                | '>'
                | '+'
                | '*'
                | '/'
                | '\\'
                | '^'
                | ','
        )
    })
}

fn parse_with(src: &str, path_only: bool) -> Result<Expr> {
    let tokens = tokenize(src)?;
    let mut p = Parser {
        tokens: &tokens,
        i: 0,
        path_only,
    };
    let expr = p.parse_bp(0)?;
    p.skip_eof()?;
    Ok(expr)
}

struct Parser<'a> {
    tokens: &'a [Token],
    i: usize,
    path_only: bool,
}

impl Parser<'_> {
    fn peek(&self) -> &TokenKind {
        self.tokens
            .get(self.i)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn at(&self) -> usize {
        self.tokens.get(self.i).map(|t| t.start).unwrap_or(0)
    }

    fn bump(&mut self) -> TokenKind {
        let k = self.peek().clone();
        if !matches!(k, TokenKind::Eof) {
            self.i += 1;
        }
        k
    }

    fn skip_eof(&self) -> Result<()> {
        match self.peek() {
            TokenKind::Eof => Ok(()),
            k => Err(invalid(
                self.at(),
                format!("unexpected token after expression: {k:?}"),
            )),
        }
    }

    fn parse_bp(&mut self, min_bp: u8) -> Result<Expr> {
        let mut left = self.prefix()?;
        loop {
            if matches!(self.peek(), TokenKind::LParen) && postfix_call_bp() >= min_bp {
                left = self.parse_call(left)?;
                continue;
            }
            let Some((lbp, rbp, kind)) = infix_bp(self.peek()) else {
                break;
            };
            if lbp < min_bp {
                break;
            }
            if self.path_only && !matches!(kind, Infix::Dot | Infix::Bang) {
                break;
            }
            self.bump();
            left = match kind {
                Infix::Between => self.parse_between(left)?,
                Infix::In => self.parse_in(left)?,
                Infix::Is => self.parse_is(left)?,
                Infix::Dot | Infix::Bang => self.parse_member(left, kind == Infix::Bang)?,
                other => {
                    let right = self.parse_bp(rbp)?;
                    Expr::Binary {
                        op: other.to_op(),
                        left: Box::new(left),
                        right: Box::new(right),
                    }
                }
            };
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Expr> {
        match self.peek() {
            TokenKind::Not if !self.path_only => {
                self.bump();
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_bp(NOT_BP)?),
                })
            }
            TokenKind::Minus if !self.path_only => {
                self.bump();
                let expr = self.parse_bp(UNARY_BP)?;
                Ok(fold_neg(expr))
            }
            TokenKind::Plus if !self.path_only => {
                self.bump();
                let expr = self.parse_bp(UNARY_BP)?;
                Ok(match expr {
                    Expr::Integer(_) | Expr::Float(_) => expr,
                    other => Expr::Unary {
                        op: UnaryOp::Pos,
                        expr: Box::new(other),
                    },
                })
            }
            _ => self.atom(),
        }
    }

    fn atom(&mut self) -> Result<Expr> {
        let start = self.at();
        match self.bump() {
            TokenKind::Ident(name) => Ok(Expr::Ident {
                name,
                quoted: false,
            }),
            TokenKind::Bracket(name) => Ok(Expr::Ident { name, quoted: true }),
            TokenKind::Star => Ok(Expr::Star),
            TokenKind::Integer(n) if !self.path_only => Ok(Expr::Integer(n)),
            TokenKind::Float(n) if !self.path_only => Ok(Expr::Float(n)),
            TokenKind::String(s) if !self.path_only => Ok(Expr::Text(s)),
            TokenKind::Date(s) if !self.path_only => Ok(Expr::Date(s)),
            TokenKind::True if !self.path_only => Ok(Expr::Bool(true)),
            TokenKind::False if !self.path_only => Ok(Expr::Bool(false)),
            TokenKind::Null if !self.path_only => Ok(Expr::Null),
            TokenKind::LParen if !self.path_only => {
                let inner = self.parse_bp(0)?;
                match self.bump() {
                    TokenKind::RParen => Ok(inner),
                    _ => Err(invalid(self.at(), "expected ')'")),
                }
            }
            other => Err(invalid(
                start,
                format!("expected expression, found {other:?}"),
            )),
        }
    }

    fn parse_call(&mut self, callee: Expr) -> Result<Expr> {
        let name =
            func_name(&callee).ok_or_else(|| invalid(self.at(), "only a name can be called"))?;
        self.bump(); // (
        let mut args = Vec::new();
        if !matches!(self.peek(), TokenKind::RParen) {
            loop {
                args.push(self.parse_bp(0)?);
                match self.peek() {
                    TokenKind::Comma => {
                        self.bump();
                    }
                    TokenKind::RParen => break,
                    _ => return Err(invalid(self.at(), "expected ',' or ')' in call")),
                }
            }
        }
        match self.bump() {
            TokenKind::RParen => Ok(Expr::Call { name, args }),
            _ => Err(invalid(self.at(), "expected ')'")),
        }
    }

    fn parse_member(&mut self, left: Expr, bang: bool) -> Result<Expr> {
        let quoted = self.peek().is_bracket();
        let rhs = if matches!(self.peek(), TokenKind::Star) {
            self.bump();
            Expr::Star
        } else if let Some(name) = self.peek().as_name().map(str::to_string) {
            self.bump();
            Expr::Ident { name, quoted }
        } else {
            return Err(invalid(
                self.at(),
                if bang {
                    "expected name after '!'"
                } else {
                    "expected name after '.'"
                },
            ));
        };
        Ok(extend_path(left, bang, rhs))
    }

    fn parse_between(&mut self, left: Expr) -> Result<Expr> {
        let lo = self.parse_bp(CMP_BP + 1)?;
        if !matches!(self.peek(), TokenKind::And) {
            return Err(invalid(self.at(), "expected And after Between"));
        }
        self.bump();
        let hi = self.parse_bp(CMP_BP + 1)?;
        Ok(Expr::Binary {
            op: BinaryOp::Between,
            left: Box::new(left),
            right: Box::new(list_expr(vec![lo, hi])),
        })
    }

    fn parse_in(&mut self, left: Expr) -> Result<Expr> {
        if !matches!(self.peek(), TokenKind::LParen) {
            return Err(invalid(self.at(), "expected '(' after In"));
        }
        self.bump();
        let mut args = Vec::new();
        if !matches!(self.peek(), TokenKind::RParen) {
            loop {
                args.push(self.parse_bp(0)?);
                match self.peek() {
                    TokenKind::Comma => {
                        self.bump();
                    }
                    TokenKind::RParen => break,
                    _ => return Err(invalid(self.at(), "expected ',' or ')' in In list")),
                }
            }
        }
        match self.bump() {
            TokenKind::RParen => Ok(Expr::Binary {
                op: BinaryOp::In,
                left: Box::new(left),
                right: Box::new(list_expr(args)),
            }),
            _ => Err(invalid(self.at(), "expected ')'")),
        }
    }

    fn parse_is(&mut self, left: Expr) -> Result<Expr> {
        let negated = matches!(self.peek(), TokenKind::Not);
        if negated {
            self.bump();
        }
        let right = if matches!(self.peek(), TokenKind::Null) {
            self.bump();
            Expr::Null
        } else {
            self.parse_bp(CMP_BP + 1)?
        };
        let e = Expr::Binary {
            op: BinaryOp::Is,
            left: Box::new(left),
            right: Box::new(right),
        };
        if negated {
            Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(e),
            })
        } else {
            Ok(e)
        }
    }
}

fn fold_neg(expr: Expr) -> Expr {
    match expr {
        Expr::Integer(n) if n != i64::MIN => Expr::Integer(-n),
        Expr::Float(n) => Expr::Float(-n),
        other => Expr::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(other),
        },
    }
}

fn extend_path(left: Expr, bang: bool, rhs: Expr) -> Expr {
    match (left, bang) {
        (Expr::Qualified(mut v), false) => {
            v.push(rhs);
            Expr::Qualified(v)
        }
        (Expr::Bang(mut v), true) => {
            v.push(rhs);
            Expr::Bang(v)
        }
        (left, false) => Expr::Qualified(vec![left, rhs]),
        (left, true) => Expr::Bang(vec![left, rhs]),
    }
}

fn func_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident { name, .. } => Some(name.clone()),
        Expr::Qualified(parts) => {
            let names: Option<Vec<String>> = parts
                .iter()
                .map(|p| match p {
                    Expr::Ident { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect();
            Some(names?.join("."))
        }
        Expr::Bang(parts) => {
            let names: Option<Vec<String>> = parts
                .iter()
                .map(|p| match p {
                    Expr::Ident { name, .. } => Some(name.clone()),
                    _ => None,
                })
                .collect();
            Some(names?.join("!"))
        }
        _ => None,
    }
}

const OR_BP: u8 = 1;
const AND_BP: u8 = 3;
const NOT_BP: u8 = 4;
const CMP_BP: u8 = 5;
const CONCAT_BP: u8 = 7;
const ADD_BP: u8 = 9;
const MOD_BP: u8 = 11;
const INTDIV_BP: u8 = 13;
const MUL_BP: u8 = 15;
const UNARY_BP: u8 = 17;
const POW_LBP: u8 = 20;
const POW_RBP: u8 = 19;
const MEMBER_BP: u8 = 21;

fn postfix_call_bp() -> u8 {
    MEMBER_BP
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Infix {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Like,
    In,
    Between,
    Is,
    Concat,
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Dot,
    Bang,
}

impl Infix {
    fn to_op(self) -> BinaryOp {
        match self {
            Infix::Or => BinaryOp::Or,
            Infix::And => BinaryOp::And,
            Infix::Eq => BinaryOp::Eq,
            Infix::Ne => BinaryOp::NotEq,
            Infix::Lt => BinaryOp::Lt,
            Infix::Gt => BinaryOp::Gt,
            Infix::Le => BinaryOp::Le,
            Infix::Ge => BinaryOp::Ge,
            Infix::Like => BinaryOp::Like,
            Infix::In => BinaryOp::In,
            Infix::Between => BinaryOp::Between,
            Infix::Is => BinaryOp::Is,
            Infix::Concat => BinaryOp::Concat,
            Infix::Add => BinaryOp::Add,
            Infix::Sub => BinaryOp::Sub,
            Infix::Mul => BinaryOp::Mul,
            Infix::Div => BinaryOp::Div,
            Infix::IntDiv => BinaryOp::IntDiv,
            Infix::Mod => BinaryOp::Mod,
            Infix::Pow => BinaryOp::Pow,
            Infix::Dot | Infix::Bang => unreachable!("member access is not a binary op"),
        }
    }
}

fn infix_bp(kind: &TokenKind) -> Option<(u8, u8, Infix)> {
    let (lbp, rbp, inf) = match kind {
        TokenKind::Or => (OR_BP, OR_BP + 1, Infix::Or),
        TokenKind::And => (AND_BP, AND_BP + 1, Infix::And),
        TokenKind::Eq => (CMP_BP, CMP_BP + 1, Infix::Eq),
        TokenKind::Ne => (CMP_BP, CMP_BP + 1, Infix::Ne),
        TokenKind::Lt => (CMP_BP, CMP_BP + 1, Infix::Lt),
        TokenKind::Gt => (CMP_BP, CMP_BP + 1, Infix::Gt),
        TokenKind::Le => (CMP_BP, CMP_BP + 1, Infix::Le),
        TokenKind::Ge => (CMP_BP, CMP_BP + 1, Infix::Ge),
        TokenKind::Like => (CMP_BP, CMP_BP + 1, Infix::Like),
        TokenKind::In => (CMP_BP, CMP_BP + 1, Infix::In),
        TokenKind::Between => (CMP_BP, CMP_BP + 1, Infix::Between),
        TokenKind::Is => (CMP_BP, CMP_BP + 1, Infix::Is),
        TokenKind::Amp => (CONCAT_BP, CONCAT_BP + 1, Infix::Concat),
        TokenKind::Plus => (ADD_BP, ADD_BP + 1, Infix::Add),
        TokenKind::Minus => (ADD_BP, ADD_BP + 1, Infix::Sub),
        TokenKind::Mod => (MOD_BP, MOD_BP + 1, Infix::Mod),
        TokenKind::Backslash => (INTDIV_BP, INTDIV_BP + 1, Infix::IntDiv),
        TokenKind::Star => (MUL_BP, MUL_BP + 1, Infix::Mul),
        TokenKind::Slash => (MUL_BP, MUL_BP + 1, Infix::Div),
        TokenKind::Caret => (POW_LBP, POW_RBP, Infix::Pow),
        TokenKind::Dot => (MEMBER_BP, MEMBER_BP + 1, Infix::Dot),
        TokenKind::Bang => (MEMBER_BP, MEMBER_BP + 1, Infix::Bang),
        _ => return None,
    };
    Some((lbp, rbp, inf))
}
