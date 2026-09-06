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
    /// `(name, type code)` from a `Parameters` block.
    pub parameters: Vec<(String, String)>,
    /// `dbXxx "Name" =Value` lines (`ReturnsRecords`, `ODBCTimeout`, …).
    pub properties: Vec<(String, String)>,
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

    /// SQL for the query. `complete` is false when parts had to be left as comments
    /// (non-select operations, joins that could not be placed).
    pub fn to_sql(&self) -> (String, bool) {
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
        let entries = |kind: &str| doc.block(kind).map(|b| b.entries.clone()).unwrap_or_default();
        for (k, v) in entries("InputTables") {
            match k.as_str() {
                "Name" => q.tables.push((v, None)),
                "Alias" => {
                    if let Some(last) = q.tables.last_mut() {
                        last.1 = Some(v);
                    }
                }
                _ => {}
            }
        }
        let mut pending_alias = None;
        for (k, v) in entries("OutputColumns") {
            match k.as_str() {
                "Alias" => pending_alias = Some(v),
                "Expression" => q.columns.push(OutputColumn { expression: v, alias: pending_alias.take() }),
                _ => {}
            }
        }
        let (mut l, mut r, mut e) = (None, None, None);
        for (k, v) in entries("Joins") {
            match k.as_str() {
                "LeftTable" => l = Some(v),
                "RightTable" => r = Some(v),
                "Expression" => e = Some(v),
                "Flag" => {
                    if let (Some(lt), Some(rt), Some(ex)) = (l.take(), r.take(), e.take()) {
                        q.joins.push(Join { left_table: lt, right_table: rt, expression: ex, flag: v.parse().unwrap_or(1) });
                    }
                }
                _ => {}
            }
        }
        q.group_by = entries("Groups").into_iter().filter(|(k, _)| k == "Expression").map(|(_, v)| v).collect();
        let mut last: Option<String> = None;
        for (k, v) in entries("OrderBy") {
            match k.as_str() {
                "Expression" => {
                    if let Some(x) = last.take() {
                        q.order_by.push((x, false));
                    }
                    last = Some(v);
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
                "Name" => pname = Some(v),
                "Type" => {
                    if let Some(n) = pname.take() {
                        q.parameters.push((n, v));
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

    pub fn to_sql(&self) -> (String, bool) {
        let mut complete = true;
        let table_ref = |name: &str| match self.tables.iter().find(|(n, _)| n == name) {
            Some((n, Some(a))) => format!("{} AS {}", bracket(n), bracket(a)),
            _ => bracket(name),
        };
        if self.operation != Some(Operation::Select) {
            let mut sql = format!("-- {:?} query: not a select; definition follows for manual rewriting\n", self.operation.unwrap_or(Operation::Unknown(0)));
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
            return (sql, false);
        }
        let join_word = |flag: u32, swapped: bool| match (flag, swapped) {
            (2, false) | (3, true) => "LEFT JOIN",
            (3, false) | (2, true) => "RIGHT JOIN",
            _ => "INNER JOIN",
        };
        let mut from = String::new();
        let mut joined: Vec<String> = Vec::new();
        for j in &self.joins {
            let has_l = joined.contains(&j.left_table);
            let has_r = joined.contains(&j.right_table);
            if from.is_empty() {
                from = format!("{} {} {} ON {}", table_ref(&j.left_table), join_word(j.flag, false), table_ref(&j.right_table), j.expression);
                joined.push(j.left_table.clone());
                joined.push(j.right_table.clone());
            } else if has_l && !has_r {
                from = format!("({from}) {} {} ON {}", join_word(j.flag, false), table_ref(&j.right_table), j.expression);
                joined.push(j.right_table.clone());
            } else if has_r && !has_l {
                from = format!("({from}) {} {} ON {}", join_word(j.flag, true), table_ref(&j.left_table), j.expression);
                joined.push(j.left_table.clone());
            } else {
                complete = false;
                from.push_str(&format!(" /* unplaced join: {} {} {} ON {} */", j.left_table, join_word(j.flag, false), j.right_table, j.expression));
            }
        }
        let mut from_parts = Vec::new();
        if !from.is_empty() {
            from_parts.push(from);
        }
        for (n, _) in self.tables.iter().filter(|(n, _)| !joined.contains(n)) {
            from_parts.push(table_ref(n));
        }
        let columns: Vec<String> = self
            .columns
            .iter()
            .map(|c| match &c.alias {
                Some(a) => format!("{} AS {}", c.expression, bracket(a)),
                None => c.expression.clone(),
            })
            .collect();
        let mut sql = format!(
            "SELECT {}{}\n",
            if self.option & 1 == 1 { "DISTINCT " } else { "" },
            if columns.is_empty() { "*".to_string() } else { columns.join(", ") }
        );
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
        (sql, complete)
    }
}

fn bracket(name: &str) -> String {
    if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') { name.to_string() } else { format!("[{name}]") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_with_join_where_order() {
        let text = "Operation =1\nOption =0\nWhere =\"(((PV.VendorID)=[Parent]![VendorID]))\"\nBegin InputTables\n    Name =\"Products\"\n    Name =\"ProductVendors\"\n    Alias =\"PV\"\nEnd\nBegin OutputColumns\n    Expression =\"Products.ProductID\"\n    Alias =\"Cost\"\n    Expression =\"Products.StandardUnitCost\"\nEnd\nBegin Joins\n    LeftTable =\"Products\"\n    RightTable =\"ProductVendors\"\n    Expression =\"Products.ProductID = PV.ProductID\"\n    Flag =2\nEnd\nBegin OrderBy\n    Expression =\"Products.ProductName\"\n    Flag =1\nEnd\ndbBoolean \"ReturnsRecords\" =\"-1\"\n";
        let q = Query::parse("q", text.to_string());
        assert_eq!(q.definition.operation, Some(Operation::Select));
        assert_eq!(q.definition.tables[1], ("ProductVendors".to_string(), Some("PV".to_string())));
        assert_eq!(q.definition.properties, vec![("ReturnsRecords".to_string(), "-1".to_string())]);
        let (sql, complete) = q.to_sql();
        assert!(complete);
        assert_eq!(sql, "SELECT Products.ProductID, Products.StandardUnitCost AS Cost\nFROM Products LEFT JOIN ProductVendors AS PV ON Products.ProductID = PV.ProductID\nWHERE (((PV.VendorID)=[Parent]![VendorID]))\nORDER BY Products.ProductName DESC\n;");
    }

    #[test]
    fn non_select_is_commented() {
        let q = Query::parse("q", "Operation =5\nBegin InputTables\n    Name =\"T\"\nEnd\n".to_string());
        let (sql, complete) = q.to_sql();
        assert!(!complete);
        assert!(sql.starts_with("-- Delete query"));
    }
}
