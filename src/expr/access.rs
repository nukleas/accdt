//! Render an expression back in Access's own syntax (`Display`): brackets, `"strings"`,
//! `#dates#`, `&`, bang paths. Used by descriptions and by anything that shows the client
//! what their form or macro says without translating it.

use std::fmt;

use super::{BinaryOp, Expr, UnaryOp};

fn needs_brackets(name: &str) -> bool {
    name.is_empty()
        || name.chars().next().is_some_and(|c| c.is_ascii_digit())
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn ident(name: &str, quoted: bool) -> String {
    if quoted || needs_brackets(name) {
        format!("[{name}]")
    } else {
        name.to_string()
    }
}

/// Lower number binds looser; children with a looser or equal operator get parentheses.
fn prec(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Or => 1,
        BinaryOp::And => 2,
        BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::Le
        | BinaryOp::Ge
        | BinaryOp::Like
        | BinaryOp::In
        | BinaryOp::Between
        | BinaryOp::Is => 3,
        BinaryOp::Concat | BinaryOp::Amp => 4,
        BinaryOp::Add | BinaryOp::Sub => 5,
        BinaryOp::Mod => 6,
        BinaryOp::IntDiv => 7,
        BinaryOp::Mul | BinaryOp::Div => 8,
        BinaryOp::Pow => 10,
    }
}

fn op_text(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Concat | BinaryOp::Amp => " & ",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::IntDiv => "\\",
        BinaryOp::Mod => " Mod ",
        BinaryOp::Pow => "^",
        BinaryOp::Eq => "=",
        BinaryOp::NotEq => "<>",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Le => "<=",
        BinaryOp::Ge => ">=",
        BinaryOp::And => " And ",
        BinaryOp::Or => " Or ",
        BinaryOp::Like => " Like ",
        BinaryOp::In => " In ",
        BinaryOp::Between => " Between ",
        BinaryOp::Is => " Is ",
    }
}

fn operand(e: &Expr, parent: BinaryOp, right: bool) -> String {
    match e {
        Expr::Binary { op, .. } => {
            let same_chain = *op == parent
                && !right
                && matches!(
                    parent,
                    BinaryOp::And
                        | BinaryOp::Or
                        | BinaryOp::Concat
                        | BinaryOp::Amp
                        | BinaryOp::Add
                        | BinaryOp::Mul
                );
            if same_chain || prec(*op) > prec(parent) {
                e.to_string()
            } else {
                format!("({e})")
            }
        }
        _ => e.to_string(),
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Ident { name, quoted } => f.write_str(&ident(name, *quoted)),
            Expr::Qualified(parts) => f.write_str(
                &parts
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("."),
            ),
            Expr::Bang(parts) => f.write_str(
                &parts
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("!"),
            ),
            Expr::Integer(n) => write!(f, "{n}"),
            Expr::Float(n) => write!(f, "{n}"),
            Expr::Text(s) => write!(f, "\"{}\"", s.replace('"', "\"\"")),
            Expr::Date(s) => write!(f, "#{s}#"),
            Expr::Bool(true) => f.write_str("True"),
            Expr::Bool(false) => f.write_str("False"),
            Expr::Null => f.write_str("Null"),
            Expr::Star => f.write_str("*"),
            Expr::Unary { op, expr } => {
                if let (
                    UnaryOp::Not,
                    Expr::Binary {
                        op: BinaryOp::Is,
                        left,
                        right,
                    },
                ) = (op, expr.as_ref())
                    && matches!(right.as_ref(), Expr::Null)
                {
                    return write!(f, "{} Is Not Null", operand(left, BinaryOp::Is, false));
                }
                let inner = match expr.as_ref() {
                    Expr::Binary { .. } => format!("({expr})"),
                    _ => expr.to_string(),
                };
                match op {
                    UnaryOp::Not => write!(f, "Not {inner}"),
                    UnaryOp::Neg => write!(f, "-{inner}"),
                    UnaryOp::Pos => write!(f, "+{inner}"),
                }
            }
            Expr::Binary { op, left, right } => {
                let l = operand(left, *op, false);
                match (op, right.as_ref()) {
                    (BinaryOp::Between, Expr::Call { name, args })
                        if name.is_empty() && args.len() == 2 =>
                    {
                        write!(f, "{l} Between {} And {}", args[0], args[1])
                    }
                    (BinaryOp::In, Expr::Call { name, args }) if name.is_empty() => write!(
                        f,
                        "{l} In ({})",
                        args.iter()
                            .map(|a| a.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    _ => write!(f, "{l}{}{}", op_text(*op), operand(right, *op, true)),
                }
            }
            Expr::Call { name, args } => {
                let inner = args
                    .iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                if name.is_empty() {
                    write!(f, "({inner})")
                } else {
                    write!(f, "{name}({inner})")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::expr::parse_expr;

    #[test]
    fn round_trips_access_surface() {
        for src in [
            "IIf(IsNull([Last Name]),[Company],[Last Name] & \", \" & [First Name])",
            "([Open Projects].[Status]=\"In Progress\")",
            "Forms![Project Details]!ID",
            "Nz(DMax(\"[ID]\",\"Tasks\"),0)",
            "[End Date] Is Null Or [Status]<>\"Deferred\"",
            "[Budget]>1000 And Not [Done]",
            "[Start Date]>=#1/1/1900#",
            "[Title] Like \"a*\"",
            "[ID] In (1, 2, 3)",
            "[Cost] Between 1 And 5",
            "[Budget]-([Cost]+1)*2",
        ] {
            let e = parse_expr(src).unwrap();
            let back = e.to_string();
            let again = parse_expr(&back).unwrap();
            assert_eq!(e, again, "{src} -> {back}");
        }
        assert_eq!(
            parse_expr("Not IsNull([ID])").unwrap().to_string(),
            "Not IsNull([ID])"
        );
        assert_eq!(
            parse_expr("Not ([Owner] Is Null)").unwrap().to_string(),
            "[Owner] Is Not Null"
        );
    }
}
