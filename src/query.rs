//! Saved queries: the SaveAsText definition and SQL reconstruction.

use crate::saveastext::{self, Document};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Operation {
    Select,
    MakeTable,
    Append,
    Update,
    Delete,
    Crosstab,
    Ddl,
    PassThrough,
    Union,
    Unknown(u32),
}

impl Operation {
    fn from_code(code: &str) -> Operation {
        match code.trim() {
            "1" => Operation::Select,
            "2" => Operation::MakeTable,
            "3" => Operation::Append,
            "4" => Operation::Update,
            "5" => Operation::Delete,
            "6" => Operation::Crosstab,
            "7" => Operation::Ddl,
            "8" => Operation::PassThrough,
            "9" => Operation::Union,
            other => Operation::Unknown(other.parse().unwrap_or(0)),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct OutputColumn {
    pub expression: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Join {
    /// Table name or alias, as the definition refers to it.
    pub left_table: String,
    pub right_table: String,
    pub expression: String,
    /// 1 inner, 2 left outer, 3 right outer.
    pub flag: u32,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct QueryDef {
    pub operation: Option<Operation>,
    pub option: u32,
    /// `(table, alias)`.
    pub tables: Vec<(String, Option<String>)>,
    pub columns: Vec<OutputColumn>,
    pub joins: Vec<Join>,
    pub where_clause: Option<String>,
    pub having: Option<String>,
    pub group_by: Vec<String>,
    /// `(expression, descending)`.
    pub order_by: Vec<(String, bool)>,
    /// `(name, DAO type code)` from a `Parameters` block.
    pub parameters: Vec<(String, String)>,
    /// `dbXxx "Name" =Value` lines (`ReturnsRecords`, `ODBCTimeout`, `SQL`, …).
    pub properties: Vec<(String, String)>,
}

/// Where the SQL text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum SqlSource {
    /// Access stored the SQL text itself (union, pass-through, `TOP`, DDL, and SQL-view queries).
    Stored,
    /// Rebuilt from the query's structured definition.
    Reconstructed,
    /// Not SQL: a commented dump of the definition for manual rewriting.
    Skeleton,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Sql {
    pub text: String,
    pub source: SqlSource,
    /// False when parts had to be left as comments (a join that could not be placed).
    pub complete: bool,
}

impl Sql {
    /// True when `text` is SQL rather than a commented skeleton.
    pub fn is_executable(&self) -> bool {
        self.source != SqlSource::Skeleton && self.complete
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Query {
    pub name: String,
    pub definition: QueryDef,
    pub text: String,
}

impl Query {
    /// Parse a query from SaveAsText text.
    pub fn parse(name: &str, text: String) -> Query {
        let doc = saveastext::parse(&text);
        Query { name: name.to_string(), definition: QueryDef::from_document(&doc), text }
    }

    pub fn to_sql(&self) -> Sql {
        self.definition.to_sql()
    }
}

impl QueryDef {
    pub fn from_document(doc: &Document) -> QueryDef {
        let mut q = QueryDef {
            operation: doc.get("Operation").map(Operation::from_code),
            option: doc.get("Option").and_then(|o| o.parse().ok()).unwrap_or(0),
            where_clause: doc.get("Where").map(String::from),
            having: doc.get("Having").map(String::from),
            ..Default::default()
        };
        let entries = |kind: &str| doc.block(kind).into_iter().flat_map(|b| b.entries.iter());
        for (k, v) in entries("InputTables") {
            match k.as_str() {
                "Name" => q.tables.push((v.clone(), None)),
                "Alias" => {
                    if let Some(last) = q.tables.last_mut() {
                        last.1 = Some(v.clone());
                    }
                }
                _ => {}
            }
        }
        let mut pending_alias = None;
        for (k, v) in entries("OutputColumns") {
            match k.as_str() {
                "Alias" => pending_alias = Some(v.clone()),
                "Expression" => q.columns.push(OutputColumn { expression: v.clone(), alias: pending_alias.take() }),
                _ => {}
            }
        }
        let (mut l, mut r, mut e) = (None, None, None);
        for (k, v) in entries("Joins") {
            match k.as_str() {
                "LeftTable" => l = Some(v.clone()),
                "RightTable" => r = Some(v.clone()),
                "Expression" => e = Some(v.clone()),
                "Flag" => {
                    if let (Some(lt), Some(rt), Some(ex)) = (l.take(), r.take(), e.take()) {
                        q.joins.push(Join { left_table: lt, right_table: rt, expression: ex, flag: v.parse().unwrap_or(1) });
                    }
                }
                _ => {}
            }
        }
        q.group_by = entries("Groups").filter(|(k, _)| k == "Expression").map(|(_, v)| v.clone()).collect();
        let mut last: Option<String> = None;
        for (k, v) in entries("OrderBy") {
            match k.as_str() {
                "Expression" => {
                    if let Some(x) = last.take() {
                        q.order_by.push((x, false));
                    }
                    last = Some(v.clone());
                }
                "Flag" => {
                    if let Some(x) = last.take() {
                        q.order_by.push((x, v == "1"));
                    }
                }
                _ => {}
            }
        }
        if let Some(x) = last {
            q.order_by.push((x, false));
        }
        let mut pname = None;
        for (k, v) in entries("Parameters") {
            match k.as_str() {
                "Name" => pname = Some(v.clone()),
                "Type" => {
                    if let Some(n) = pname.take() {
                        q.parameters.push((n, v.clone()));
                    }
                }
                _ => {}
            }
        }
        q.properties = doc
            .header_entries
            .iter()
            .filter(|(k, _)| !matches!(k.as_str(), "Operation" | "Option" | "Where" | "Having"))
            .cloned()
            .collect();
        q
    }

    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// SQL text Access stored verbatim, when the query is defined by SQL rather than structure.
    pub fn stored_sql(&self) -> Option<&str> {
        self.property("SQL").map(str::trim).filter(|s| !s.is_empty())
    }

    pub fn to_sql(&self) -> Sql {
        if let Some(sql) = self.stored_sql() {
            return Sql { text: sql.to_string(), source: SqlSource::Stored, complete: true };
        }
        if self.operation != Some(Operation::Select) {
            return Sql { text: self.skeleton(), source: SqlSource::Skeleton, complete: false };
        }
        let mut complete = true;
        // A table is referred to by its alias when it has one, otherwise by its name.
        let key_of = |t: &(String, Option<String>)| t.1.clone().unwrap_or_else(|| t.0.clone());
        let table_ref = |key: &str| match self.tables.iter().find(|t| key_of(t) == key) {
            Some((n, Some(a))) => format!("{} AS {}", bracket(n), bracket(a)),
            Some((n, None)) => bracket(n),
            None => bracket(key),
        };
        let join_word = |flag: u32, swapped: bool| match (flag, swapped) {
            (2, false) | (3, true) => "LEFT JOIN",
            (3, false) | (2, true) => "RIGHT JOIN",
            _ => "INNER JOIN",
        };
        let mut from = String::new();
        let mut used: Vec<String> = Vec::new();
        for j in &self.joins {
            let has_l = used.contains(&j.left_table);
            let has_r = used.contains(&j.right_table);
            if from.is_empty() {
                from = format!("{} {} {} ON {}", table_ref(&j.left_table), join_word(j.flag, false), table_ref(&j.right_table), j.expression);
                used.push(j.left_table.clone());
                used.push(j.right_table.clone());
            } else if has_l && !has_r {
                from = format!("({from}) {} {} ON {}", join_word(j.flag, false), table_ref(&j.right_table), j.expression);
                used.push(j.right_table.clone());
            } else if has_r && !has_l {
                from = format!("({from}) {} {} ON {}", join_word(j.flag, true), table_ref(&j.left_table), j.expression);
                used.push(j.left_table.clone());
            } else {
                complete = false;
                from.push_str(&format!(" /* unplaced join: {} {} {} ON {} */", j.left_table, join_word(j.flag, false), j.right_table, j.expression));
            }
        }
        let mut from_parts = Vec::new();
        if !from.is_empty() {
            from_parts.push(from);
        }
        for t in self.tables.iter().filter(|t| !used.contains(&key_of(t))) {
            from_parts.push(table_ref(&key_of(t)));
        }
        let columns: Vec<String> = self
            .columns
            .iter()
            .map(|c| match &c.alias {
                Some(a) => format!("{} AS {}", c.expression, bracket(a)),
                None => c.expression.clone(),
            })
            .collect();
        let mut sql = String::new();
        if !self.parameters.is_empty() {
            let params: Vec<String> = self.parameters.iter().map(|(n, t)| format!("{} {}", bracket(n), parameter_type(t))).collect();
            sql.push_str(&format!("PARAMETERS {};\n", params.join(", ")));
        }
        sql.push_str(&format!(
            "SELECT {}{}\n",
            if self.option & 1 == 1 { "DISTINCT " } else { "" },
            if columns.is_empty() { "*".to_string() } else { columns.join(", ") }
        ));
        if !from_parts.is_empty() {
            sql.push_str(&format!("FROM {}\n", from_parts.join(", ")));
        }
        if let Some(w) = &self.where_clause {
            sql.push_str(&format!("WHERE {w}\n"));
        }
        if !self.group_by.is_empty() {
            sql.push_str(&format!("GROUP BY {}\n", self.group_by.join(", ")));
        }
        if let Some(h) = &self.having {
            sql.push_str(&format!("HAVING {h}\n"));
        }
        if !self.order_by.is_empty() {
            let o: Vec<String> = self.order_by.iter().map(|(e, d)| if *d { format!("{e} DESC") } else { e.clone() }).collect();
            sql.push_str(&format!("ORDER BY {}\n", o.join(", ")));
        }
        sql.push(';');
        Sql { text: sql, source: SqlSource::Reconstructed, complete }
    }

    fn skeleton(&self) -> String {
        let mut sql = format!("-- {:?} query: no stored SQL and not a select; definition follows for manual rewriting\n", self.operation.unwrap_or(Operation::Unknown(0)));
        for (n, a) in &self.tables {
            sql.push_str(&format!("-- table: {n}{}\n", a.as_ref().map(|a| format!(" AS {a}")).unwrap_or_default()));
        }
        for c in &self.columns {
            sql.push_str(&format!("-- column: {}{}\n", c.expression, c.alias.as_ref().map(|a| format!(" AS {a}")).unwrap_or_default()));
        }
        for j in &self.joins {
            sql.push_str(&format!("-- join({}): {} / {} ON {}\n", j.flag, j.left_table, j.right_table, j.expression));
        }
        if let Some(w) = &self.where_clause {
            sql.push_str(&format!("-- where: {w}\n"));
        }
        sql
    }
}

/// Access SQL type name for a DAO type code in a `Parameters` block.
fn parameter_type(code: &str) -> &'static str {
    match code.trim() {
        "1" => "Bit",
        "2" => "Byte",
        "3" => "Short",
        "4" => "Long",
        "5" => "Currency",
        "6" => "Single",
        "7" => "Double",
        "8" => "DateTime",
        "9" | "11" => "Binary",
        "10" => "Text",
        "12" => "LongText",
        "15" => "Guid",
        "20" => "Decimal",
        _ => "Text",
    }
}

const RESERVED: &[&str] = &[
    "order", "group", "date", "time", "name", "value", "key", "index", "level", "user", "table", "select", "from", "where", "by", "count", "sum", "min",
    "max", "avg", "year", "month", "day", "desc", "asc", "text", "memo", "position", "section", "size", "type", "column", "field", "note", "password",
    "percent", "procedure", "property", "references", "report", "row", "rows", "string", "unique", "update", "values", "view", "action", "add", "all",
    "alter", "and", "any", "as", "between", "case", "check", "column", "constraint", "create", "delete", "distinct", "drop", "exists", "false", "first",
    "in", "inner", "insert", "into", "is", "join", "last", "left", "like", "not", "null", "on", "option", "or", "outer", "parameters", "pivot", "right",
    "set", "some", "top", "transform", "true", "union",
];

fn bracket(name: &str) -> String {
    let plain = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if plain && !RESERVED.contains(&name.to_ascii_lowercase().as_str()) { name.to_string() } else { format!("[{name}]") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_with_aliased_joins_where_order_and_parameters() {
        let text = "Operation =1\nOption =0\nWhere =\"(((PV.VendorID)=[Parent]![VendorID]))\"\nBegin InputTables\n    Name =\"Products\"\n    Name =\"ProductVendors\"\n    Alias =\"PV\"\n    Name =\"Order\"\nEnd\nBegin OutputColumns\n    Expression =\"Products.ProductID\"\n    Alias =\"Cost\"\n    Expression =\"Products.StandardUnitCost\"\nEnd\nBegin Joins\n    LeftTable =\"Products\"\n    RightTable =\"PV\"\n    Expression =\"Products.ProductID = PV.ProductID\"\n    Flag =2\nEnd\nBegin OrderBy\n    Expression =\"Products.ProductName\"\n    Flag =1\nEnd\nBegin Parameters\n    Name =\"Which\"\n    Type =10\nEnd\ndbBoolean \"ReturnsRecords\" =\"-1\"\n";
        let q = Query::parse("q", text.to_string());
        assert_eq!(q.definition.operation, Some(Operation::Select));
        assert_eq!(q.definition.tables[1], ("ProductVendors".to_string(), Some("PV".to_string())));
        assert_eq!(q.definition.property("ReturnsRecords"), Some("-1"));
        let sql = q.to_sql();
        assert!(sql.complete && sql.source == SqlSource::Reconstructed);
        assert_eq!(
            sql.text,
            "PARAMETERS Which Text;\nSELECT Products.ProductID, Products.StandardUnitCost AS Cost\nFROM Products LEFT JOIN ProductVendors AS PV ON Products.ProductID = PV.ProductID, [Order]\nWHERE (((PV.VendorID)=[Parent]![VendorID]))\nORDER BY Products.ProductName DESC\n;"
        );
    }

    #[test]
    fn stored_sql_wins() {
        let q = Query::parse("q", "dbMemo \"SQL\" =\"SELECT TOP 20 qryOrderList.* FROM qryOrderList;\\015\\012\"\ndbBoolean \"ReturnsRecords\" =\"-1\"\n".to_string());
        let sql = q.to_sql();
        assert_eq!(sql.source, SqlSource::Stored);
        assert_eq!(sql.text, "SELECT TOP 20 qryOrderList.* FROM qryOrderList;");
        assert!(sql.is_executable());
    }

    #[test]
    fn non_select_without_sql_is_a_skeleton() {
        let q = Query::parse("q", "Operation =5\nBegin InputTables\n    Name =\"T\"\nEnd\n".to_string());
        let sql = q.to_sql();
        assert_eq!(sql.source, SqlSource::Skeleton);
        assert!(!sql.is_executable());
        assert!(sql.text.starts_with("-- Delete query"));
    }
}
