//! Tokens for Access / Jet expressions.

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    Ident(String),
    Bracket(String),
    String(String),
    Integer(i64),
    Float(f64),
    Date(String),
    True,
    False,
    Null,
    Bang,
    Dot,
    Amp,
    Plus,
    Minus,
    Star,
    Slash,
    Backslash,
    Caret,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    LParen,
    RParen,
    Comma,
    And,
    Or,
    Not,
    Mod,
    Like,
    In,
    Between,
    Is,
    Eof,
}

impl TokenKind {
    /// Keyword or ident after `.` / `!`, where `And` is a name not an operator.
    pub(crate) fn as_name(&self) -> Option<&str> {
        match self {
            TokenKind::Ident(s) | TokenKind::Bracket(s) => Some(s),
            TokenKind::And => Some("And"),
            TokenKind::Or => Some("Or"),
            TokenKind::Not => Some("Not"),
            TokenKind::Mod => Some("Mod"),
            TokenKind::Like => Some("Like"),
            TokenKind::In => Some("In"),
            TokenKind::Between => Some("Between"),
            TokenKind::Is => Some("Is"),
            TokenKind::True => Some("True"),
            TokenKind::False => Some("False"),
            TokenKind::Null => Some("Null"),
            _ => None,
        }
    }

    pub(crate) fn is_bracket(&self) -> bool {
        matches!(self, TokenKind::Bracket(_))
    }
}

pub(crate) fn tokenize(src: &str) -> crate::Result<Vec<Token>> {
    let mut lex = Lexer { src, pos: 0 };
    let mut out = Vec::new();
    loop {
        let t = lex.next_token()?;
        let eof = t.kind == TokenKind::Eof;
        out.push(t);
        if eof {
            break;
        }
    }
    Ok(out)
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl Lexer<'_> {
    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek2(&self) -> Option<char> {
        let mut c = self.src[self.pos..].chars();
        c.next()?;
        c.next()
    }

    fn bump(&mut self) -> Option<char> {
        let mut c = self.src[self.pos..].chars();
        let ch = c.next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn next_token(&mut self) -> crate::Result<Token> {
        self.skip_ws();
        let start = self.pos;
        let Some(ch) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                start,
            });
        };
        let kind = match ch {
            '"' => self.string()?,
            '[' => self.bracket()?,
            '#' => self.date()?,
            '!' => {
                self.bump();
                TokenKind::Bang
            }
            '.' => {
                if self.peek2().is_some_and(|c| c.is_ascii_digit()) {
                    self.number()?
                } else {
                    self.bump();
                    TokenKind::Dot
                }
            }
            '&' => {
                self.bump();
                TokenKind::Amp
            }
            '+' => {
                self.bump();
                TokenKind::Plus
            }
            '-' => {
                self.bump();
                TokenKind::Minus
            }
            '*' => {
                self.bump();
                TokenKind::Star
            }
            '/' => {
                self.bump();
                TokenKind::Slash
            }
            '\\' => {
                self.bump();
                TokenKind::Backslash
            }
            '^' => {
                self.bump();
                TokenKind::Caret
            }
            '=' => {
                self.bump();
                TokenKind::Eq
            }
            '(' => {
                self.bump();
                TokenKind::LParen
            }
            ')' => {
                self.bump();
                TokenKind::RParen
            }
            ',' => {
                self.bump();
                TokenKind::Comma
            }
            '<' => {
                self.bump();
                match self.peek() {
                    Some('>') => {
                        self.bump();
                        TokenKind::Ne
                    }
                    Some('=') => {
                        self.bump();
                        TokenKind::Le
                    }
                    _ => TokenKind::Lt,
                }
            }
            '>' => {
                self.bump();
                if self.peek() == Some('=') {
                    self.bump();
                    TokenKind::Ge
                } else {
                    TokenKind::Gt
                }
            }
            '0'..='9' => self.number()?,
            c if is_ident_start(c) => self.ident(),
            _ => {
                return Err(super::invalid(start, format!("unexpected {ch:?}")));
            }
        };
        Ok(Token { kind, start })
    }

    fn string(&mut self) -> crate::Result<TokenKind> {
        let start = self.pos;
        self.bump(); // opening "
        let mut s = String::new();
        loop {
            match self.bump() {
                Some('"') => {
                    if self.peek() == Some('"') {
                        self.bump();
                        s.push('"');
                    } else {
                        return Ok(TokenKind::String(s));
                    }
                }
                Some(c) => s.push(c),
                None => return Err(super::invalid(start, "unterminated string")),
            }
        }
    }

    fn bracket(&mut self) -> crate::Result<TokenKind> {
        let start = self.pos;
        self.bump(); // [
        let mut s = String::new();
        loop {
            match self.bump() {
                Some(']') => {
                    if self.peek() == Some(']') {
                        self.bump();
                        s.push(']');
                    } else {
                        return Ok(TokenKind::Bracket(s));
                    }
                }
                Some(c) => s.push(c),
                None => return Err(super::invalid(start, "unterminated [identifier]")),
            }
        }
    }

    fn date(&mut self) -> crate::Result<TokenKind> {
        let start = self.pos;
        self.bump(); // #
        let mut s = String::new();
        loop {
            match self.bump() {
                Some('#') => return Ok(TokenKind::Date(s)),
                Some(c) => s.push(c),
                None => return Err(super::invalid(start, "unterminated #date#")),
            }
        }
    }

    fn number(&mut self) -> crate::Result<TokenKind> {
        let start = self.pos;
        let mut float = false;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        if self.peek() == Some('.') && self.peek2().is_some_and(|c| c.is_ascii_digit()) {
            float = true;
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        if self.peek().is_some_and(|c| c == 'e' || c == 'E') {
            float = true;
            self.bump();
            if self.peek().is_some_and(|c| c == '+' || c == '-') {
                self.bump();
            }
            if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return Err(super::invalid(start, "exponent has no digits"));
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
        }
        let text = &self.src[start..self.pos];
        if float {
            text.parse::<f64>()
                .map(TokenKind::Float)
                .map_err(|_| super::invalid(start, format!("invalid float {text}")))
        } else {
            match text.parse::<i64>() {
                Ok(n) => Ok(TokenKind::Integer(n)),
                Err(_) => text
                    .parse::<f64>()
                    .map(TokenKind::Float)
                    .map_err(|_| super::invalid(start, format!("invalid number {text}"))),
            }
        }
    }

    fn ident(&mut self) -> TokenKind {
        let start = self.pos;
        while self.peek().is_some_and(is_ident_part) {
            self.bump();
        }
        let text = &self.src[start..self.pos];
        match text.to_ascii_lowercase().as_str() {
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "mod" => TokenKind::Mod,
            "like" => TokenKind::Like,
            "in" => TokenKind::In,
            "between" => TokenKind::Between,
            "is" => TokenKind::Is,
            _ => TokenKind::Ident(text.to_string()),
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
