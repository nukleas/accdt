use super::{BinaryOp, Expr, Param, UnaryOp, list_args};

pub struct RenderOpts {
    pub ident_quote: char,
    pub concat: &'static str,
    pub true_lit: &'static str,
    pub like_wildcard: fn(&str) -> String,
}

impl RenderOpts {
    pub fn sqlite() -> RenderOpts {
        RenderOpts {
            ident_quote: '"',
            concat: "||",
            true_lit: "1",
            like_wildcard: access_like_to_sql,
        }
    }
}

impl Default for RenderOpts {
    fn default() -> RenderOpts {
        RenderOpts::sqlite()
    }
}

/// Access `Like` wildcards `*` `?` `#` → SQL `%` `_` `_`; `%` `_` `\` are escaped.
pub fn access_like_to_sql(pat: &str) -> String {
    let mut out = String::new();
    for c in pat.chars() {
        match c {
            '*' => out.push('%'),
            '?' | '#' => out.push('_'),
            '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            '\\' => out.push_str("\\\\"),
            _ => out.push(c),
        }
    }
    out
}

pub(crate) fn to_sql(expr: &Expr, opts: &RenderOpts, sqlite: bool) -> String {
    render(expr, opts, sqlite, 0)
}

fn render(expr: &Expr, opts: &RenderOpts, sqlite: bool, parent: u8) -> String {
    if sqlite {
        if let Some(p) = param_of(expr) {
            return placeholder(&p);
        }
        if let Some(s) = lower_call(expr, opts, sqlite) {
            return wrap(s, atom_prec(), parent);
        }
    }
    match expr {
        Expr::Ident { name, .. } => quote_ident(name, opts),
        Expr::Star => "*".into(),
        Expr::Integer(n) => n.to_string(),
        Expr::Float(n) => {
            let s = n.to_string();
            if s.contains('.') || s.contains('e') || s.contains('E') {
                s
            } else {
                format!("{s}.0")
            }
        }
        Expr::Text(s) => sql_string(s),
        Expr::Date(s) => render_date(s, sqlite),
        Expr::Bool(true) => opts.true_lit.to_string(),
        Expr::Bool(false) => false_lit(opts).to_string(),
        Expr::Null => "NULL".into(),
        Expr::Qualified(parts) | Expr::Bang(parts) => parts
            .iter()
            .map(|p| render(p, opts, sqlite, MEMBER_PREC))
            .collect::<Vec<_>>()
            .join("."),
        Expr::Unary { op, expr } => {
            let bp = unary_prec(*op);
            let inner = render(expr, opts, sqlite, bp);
            let s = match op {
                UnaryOp::Not => {
                    if let Expr::Binary {
                        op: BinaryOp::Is,
                        left,
                        right,
                    } = expr.as_ref()
                        && matches!(right.as_ref(), Expr::Null)
                    {
                        return wrap(
                            format!("{} IS NOT NULL", render(left, opts, sqlite, CMP_PREC)),
                            CMP_PREC,
                            parent,
                        );
                    }
                    format!("NOT {inner}")
                }
                UnaryOp::Neg => format!("-{inner}"),
                UnaryOp::Pos => format!("+{inner}"),
            };
            wrap(s, bp, parent)
        }
        Expr::Binary { op, left, right } => render_binary(*op, left, right, opts, sqlite, parent),
        Expr::Call { name, args } if name.is_empty() => format!(
            "({})",
            args.iter()
                .map(|a| render(a, opts, sqlite, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Call { name, args } => {
            let inner = args
                .iter()
                .map(|a| render(a, opts, sqlite, 0))
                .collect::<Vec<_>>()
                .join(", ");
            let fname = if sqlite {
                sqlite_func_name(name)
            } else {
                name.clone()
            };
            format!("{fname}({inner})")
        }
    }
}

fn render_binary(
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    opts: &RenderOpts,
    sqlite: bool,
    parent: u8,
) -> String {
    let bp = binary_prec(op);
    match op {
        BinaryOp::Between => {
            let bounds = list_args(right).filter(|a| a.len() == 2);
            let s = if let Some([lo, hi]) = bounds {
                format!(
                    "{} BETWEEN {} AND {}",
                    render(left, opts, sqlite, bp),
                    render(lo, opts, sqlite, bp),
                    render(hi, opts, sqlite, bp)
                )
            } else {
                format!(
                    "{} BETWEEN {}",
                    render(left, opts, sqlite, bp),
                    render(right, opts, sqlite, bp)
                )
            };
            wrap(s, bp, parent)
        }
        BinaryOp::In => {
            let list = match list_args(right) {
                Some(args) => format!(
                    "({})",
                    args.iter()
                        .map(|a| render(a, opts, sqlite, 0))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                None => render(right, opts, sqlite, 0),
            };
            wrap(
                format!("{} IN {list}", render(left, opts, sqlite, bp)),
                bp,
                parent,
            )
        }
        BinaryOp::Is => wrap(
            format!(
                "{} IS {}",
                render(left, opts, sqlite, bp),
                render(right, opts, sqlite, bp)
            ),
            bp,
            parent,
        ),
        BinaryOp::Like => {
            let (pat, escape) = match right {
                Expr::Text(s) if sqlite => {
                    let converted = (opts.like_wildcard)(s);
                    let esc = converted.contains('\\');
                    (sql_string(&converted), esc)
                }
                _ => (render(right, opts, sqlite, bp), false),
            };
            let mut s = format!("{} LIKE {pat}", render(left, opts, sqlite, bp));
            if escape {
                s.push_str(" ESCAPE '\\'");
            }
            wrap(s, bp, parent)
        }
        BinaryOp::Concat | BinaryOp::Amp => wrap(
            format!(
                "{} {} {}",
                render(left, opts, sqlite, bp),
                if sqlite { opts.concat } else { "&" },
                render(right, opts, sqlite, right_prec(op, bp))
            ),
            bp,
            parent,
        ),
        BinaryOp::IntDiv if sqlite => wrap(
            format!(
                "CAST({} AS INTEGER) / CAST({} AS INTEGER)",
                render(left, opts, sqlite, 0),
                render(right, opts, sqlite, 0)
            ),
            bp,
            parent,
        ),
        BinaryOp::Pow if sqlite => format!(
            "power({}, {})",
            render(left, opts, sqlite, 0),
            render(right, opts, sqlite, 0)
        ),
        BinaryOp::Mod if sqlite => wrap(
            format!(
                "{} % {}",
                render(left, opts, sqlite, bp),
                render(right, opts, sqlite, right_prec(op, bp))
            ),
            bp,
            parent,
        ),
        _ => wrap(
            format!(
                "{} {} {}",
                render(left, opts, sqlite, bp),
                sql_op(op),
                render(right, opts, sqlite, right_prec(op, bp))
            ),
            bp,
            parent,
        ),
    }
}

fn lower_call(expr: &Expr, opts: &RenderOpts, sqlite: bool) -> Option<String> {
    let Expr::Call { name, args } = expr else {
        return None;
    };
    if name.is_empty() {
        return None;
    }
    let n = name.to_ascii_lowercase();
    let a = |i: usize| args.get(i).map(|e| render(e, opts, sqlite, 0));
    match n.as_str() {
        "nz" => match args.len() {
            1 => Some(format!("COALESCE({}, '')", a(0)?)),
            _ => Some(format!(
                "COALESCE({})",
                args.iter()
                    .map(|e| render(e, opts, sqlite, 0))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        },
        "iif" if args.len() >= 3 => Some(format!(
            "CASE WHEN {} THEN {} ELSE {} END",
            a(0)?,
            a(1)?,
            a(2)?
        )),
        "isnull" if args.len() == 1 => Some(format!("{} IS NULL", a(0)?)),
        "ccur" | "cdbl" | "csng" if args.len() == 1 => Some(format!("CAST({} AS REAL)", a(0)?)),
        "clng" | "cint" | "cbyte" if args.len() == 1 => Some(format!("CAST({} AS INTEGER)", a(0)?)),
        "cstr" | "cvar" if args.len() == 1 => Some(format!("CAST({} AS TEXT)", a(0)?)),
        "ucase" if args.len() == 1 => Some(format!("upper({})", a(0)?)),
        "lcase" if args.len() == 1 => Some(format!("lower({})", a(0)?)),
        "left" if args.len() == 2 => Some(format!("substr({}, 1, {})", a(0)?, a(1)?)),
        "right" if args.len() == 2 => Some(format!("substr({}, -{})", a(0)?, a(1)?)),
        "mid" if args.len() == 2 => Some(format!("substr({}, {})", a(0)?, a(1)?)),
        "mid" if args.len() >= 3 => Some(format!("substr({}, {}, {})", a(0)?, a(1)?, a(2)?)),
        "replace" => Some(format!(
            "replace({})",
            args.iter()
                .map(|e| render(e, opts, sqlite, 0))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        "len" if args.len() == 1 => Some(format!("length({})", a(0)?)),
        "trim" if args.len() == 1 => Some(format!("trim({})", a(0)?)),
        "ltrim" if args.len() == 1 => Some(format!("ltrim({})", a(0)?)),
        "rtrim" if args.len() == 1 => Some(format!("rtrim({})", a(0)?)),
        "date" if args.is_empty() => Some("date('now')".into()),
        "time" if args.is_empty() => Some("time('now')".into()),
        "now" if args.is_empty() => Some("datetime('now')".into()),
        _ => None,
    }
}

fn sqlite_func_name(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "sum" => "SUM".into(),
        "count" => "COUNT".into(),
        "avg" => "AVG".into(),
        "min" => "MIN".into(),
        "max" => "MAX".into(),
        "first" => "MIN".into(),
        "last" => "MAX".into(),
        _ => name.to_string(),
    }
}

fn render_date(body: &str, sqlite: bool) -> String {
    if !sqlite {
        return format!("#{body}#");
    }
    let t = body.trim();
    if let Some(iso) = us_date_to_iso(t) {
        if iso.len() > 10 {
            format!("datetime('{iso}')")
        } else {
            format!("date('{iso}')")
        }
    } else if t.contains('-') || t.contains(':') {
        if t.contains(' ') || t.contains(':') {
            format!("datetime('{t}')")
        } else {
            format!("date('{t}')")
        }
    } else {
        format!("'{t}'")
    }
}

fn us_date_to_iso(s: &str) -> Option<String> {
    let (date, time) = match s.split_once(' ') {
        Some((d, t)) => (d, Some(t)),
        None => (s, None),
    };
    let mut parts = date.split('/');
    let a = parts.next()?.parse::<u32>().ok()?;
    let b = parts.next()?.parse::<u32>().ok()?;
    let c = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let (m, d, y) = if a > 12 { (b, a, c) } else { (a, b, c) };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if y < 100 { 2000 + y } else { y };
    let mut out = format!("{y:04}-{m:02}-{d:02}");
    if let Some(t) = time {
        out.push(' ');
        out.push_str(t);
    }
    Some(out)
}

pub(crate) fn param_of(expr: &Expr) -> Option<Param> {
    let parts = match expr {
        Expr::Bang(p) | Expr::Qualified(p) if p.len() >= 2 => p,
        _ => return None,
    };
    let col = ident_name(&parts[0])?;
    let rest = || join_names(&parts[1..]);
    match col.to_ascii_lowercase().as_str() {
        "forms" if parts.len() >= 3 => Some(Param::Form {
            form: ident_name(&parts[1])?,
            control: join_names(&parts[2..])?,
        }),
        "forms" => Some(Param::Form {
            form: ident_name(&parts[1])?,
            control: String::new(),
        }),
        "tempvars" => Some(Param::TempVar(rest()?)),
        "parent" => Some(Param::Parent(rest()?)),
        "screen" => Some(Param::Screen(rest()?)),
        "form" => Some(Param::Control(rest()?)),
        _ if matches!(expr, Expr::Bang(_)) => Some(Param::Control(
            parts
                .iter()
                .filter_map(ident_name)
                .collect::<Vec<_>>()
                .join("!"),
        )),
        _ => None,
    }
}

fn ident_name(e: &Expr) -> Option<String> {
    match e {
        Expr::Ident { name, .. } => Some(name.clone()),
        _ => None,
    }
}

fn join_names(parts: &[Expr]) -> Option<String> {
    let names: Option<Vec<String>> = parts.iter().map(ident_name).collect();
    Some(names?.join("."))
}

fn placeholder(p: &Param) -> String {
    let raw = match p {
        Param::Form { form, control } if control.is_empty() => format!(":forms_{form}"),
        Param::Form { form, control } => format!(":forms_{form}_{control}"),
        Param::TempVar(n) => format!(":tempvar_{n}"),
        Param::Parent(n) => format!(":parent_{n}"),
        Param::Screen(n) => format!(":screen_{n}"),
        Param::Control(n) => format!(":control_{n}"),
        Param::Domain { func, .. } => format!(":domain_{func}"),
    };
    sanitize_placeholder(&raw)
}

fn sanitize_placeholder(s: &str) -> String {
    let mut out = String::new();
    let mut us = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == ':' {
            out.push(c);
            us = false;
        } else if !us {
            out.push('_');
            us = true;
        }
    }
    out.trim_end_matches('_').to_string()
}

fn quote_ident(name: &str, opts: &RenderOpts) -> String {
    let q = opts.ident_quote;
    if q == '\0' {
        return name.to_string();
    }
    let escaped = name.replace(q, &format!("{q}{q}"));
    format!("{q}{escaped}{q}")
}

fn sql_string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn false_lit(opts: &RenderOpts) -> &'static str {
    if opts.true_lit == "1" { "0" } else { "FALSE" }
}

fn sql_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::IntDiv => "\\",
        BinaryOp::Mod => "Mod",
        BinaryOp::Pow => "^",
        BinaryOp::Eq => "=",
        BinaryOp::NotEq => "<>",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::Le => "<=",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "AND",
        BinaryOp::Or => "OR",
        BinaryOp::Concat | BinaryOp::Amp => "&",
        BinaryOp::Like => "LIKE",
        BinaryOp::In => "IN",
        BinaryOp::Between => "BETWEEN",
        BinaryOp::Is => "IS",
    }
}

fn wrap(s: String, bp: u8, parent: u8) -> String {
    if bp < parent { format!("({s})") } else { s }
}

fn right_prec(op: BinaryOp, bp: u8) -> u8 {
    // left-associative: parenthesize the right child at equal precedence
    if matches!(op, BinaryOp::Pow) {
        bp
    } else {
        bp + 1
    }
}

const MEMBER_PREC: u8 = 21;
const CMP_PREC: u8 = 5;
const ATOM: u8 = 30;

fn atom_prec() -> u8 {
    ATOM
}

fn unary_prec(op: UnaryOp) -> u8 {
    match op {
        UnaryOp::Not => 4,
        UnaryOp::Neg | UnaryOp::Pos => 17,
    }
}

fn binary_prec(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Or => 1,
        BinaryOp::And => 3,
        BinaryOp::Eq
        | BinaryOp::NotEq
        | BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::Le
        | BinaryOp::Ge
        | BinaryOp::Like
        | BinaryOp::In
        | BinaryOp::Between
        | BinaryOp::Is => 5,
        BinaryOp::Concat | BinaryOp::Amp => 7,
        BinaryOp::Add | BinaryOp::Sub => 9,
        BinaryOp::Mod => 11,
        BinaryOp::IntDiv => 13,
        BinaryOp::Mul | BinaryOp::Div => 15,
        BinaryOp::Pow => 20,
    }
}
