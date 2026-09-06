//! Access / Jet expressions: lexer, Pratt parser, and a SQLite renderer.

mod access;
mod lexer;
mod parser;
mod render;

use crate::error::Error;

pub use parser::{
    parse_control_source, parse_default_value, parse_expr, parse_filter_string, parse_ident_path,
};
pub use render::{RenderOpts, access_like_to_sql};

/// A parsed Access expression.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Expr {
    Ident {
        name: String,
        quoted: bool,
    },
    Qualified(Vec<Expr>),
    Bang(Vec<Expr>),
    Integer(i64),
    Float(f64),
    Text(String),
    /// Raw body of `#…#` (no surrounding hashes).
    Date(String),
    Bool(bool),
    Null,
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
    Star,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum BinaryOp {
    Concat,
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    Like,
    In,
    Between,
    Is,
    /// Same as [`BinaryOp::Concat`]; `&` is parsed as [`BinaryOp::Concat`].
    Amp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum UnaryOp {
    Not,
    Neg,
    Pos,
}

/// A runtime value the SQLite renderer turns into a `:placeholder`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Param {
    Form { form: String, control: String },
    TempVar(String),
    Parent(String),
    Screen(String),
    Control(String),
    Domain { func: String, expr: Box<Expr> },
}

impl Expr {
    pub fn to_sql(&self, opts: &RenderOpts) -> String {
        render::to_sql(self, opts, false)
    }

    /// `Nz` → `COALESCE`, `IIf` → `CASE`, `Forms!` → `:param`, Access strings → SQL strings.
    pub fn to_sqlite(&self) -> String {
        render::to_sql(self, &RenderOpts::sqlite(), true)
    }

    pub fn parameters(&self) -> Vec<Param> {
        let mut out = Vec::new();
        collect_params(self, &mut out);
        out
    }
}

fn collect_params(expr: &Expr, out: &mut Vec<Param>) {
    if let Some(p) = render::param_of(expr) {
        if !out.iter().any(|q| q == &p) {
            out.push(p);
        }
        return;
    }
    match expr {
        Expr::Qualified(parts) | Expr::Bang(parts) => {
            for p in parts {
                collect_params(p, out);
            }
        }
        Expr::Unary { expr, .. } => collect_params(expr, out),
        Expr::Binary { left, right, .. } => {
            collect_params(left, out);
            collect_params(right, out);
        }
        Expr::Call { args, .. } => {
            for a in args {
                collect_params(a, out);
            }
        }
        _ => {}
    }
}

/// `In` / `Between` pack their extra operands as a nameless call on the right.
pub(crate) fn list_expr(args: Vec<Expr>) -> Expr {
    Expr::Call {
        name: String::new(),
        args,
    }
}

pub(crate) fn list_args(expr: &Expr) -> Option<&[Expr]> {
    match expr {
        Expr::Call { name, args } if name.is_empty() => Some(args),
        _ => None,
    }
}

pub(crate) fn invalid(at: usize, reason: impl std::fmt::Display) -> Error {
    Error::Invalid {
        part: "expr".into(),
        reason: format!("byte {at}: {reason}"),
    }
}
