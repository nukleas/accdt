//! Tables: schema from the object's XML Schema part, rows from its sample-data part.

use crate::text::{self, unescape_xml_name};

const XSD: &str = "http://www.w3.org/2001/XMLSchema";
const OD: &str = "urn:schemas-microsoft-com:officedata";

/// Access field types as written in `od:jetType`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum JetType {
    AutoNumber,
    Text,
    Memo,
    Byte,
    Integer,
    LongInteger,
    Single,
    Double,
    Currency,
    Decimal,
    DateTime,
    YesNo,
    OleObject,
    Hyperlink,
    /// Attachment or multi-valued field; see `Column::complex_type`.
    Complex,
    Guid,
    Other(String),
}

impl JetType {
    fn parse(v: &str) -> JetType {
        match v.to_ascii_lowercase().as_str() {
            "autonumber" => JetType::AutoNumber,
            "text" => JetType::Text,
            "memo" => JetType::Memo,
            "byte" => JetType::Byte,
            "integer" => JetType::Integer,
            "longinteger" => JetType::LongInteger,
            "single" => JetType::Single,
            "double" => JetType::Double,
            "currency" => JetType::Currency,
            "decimal" => JetType::Decimal,
            "datetime" => JetType::DateTime,
            "yesno" => JetType::YesNo,
            "oleobject" => JetType::OleObject,
            "hyperlink" => JetType::Hyperlink,
            "complex" => JetType::Complex,
            "replicationid" | "guid" => JetType::Guid,
            other => JetType::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Column {
    pub name: String,
    pub jet_type: JetType,
    /// `od:sqlSType` (`int`, `nvarchar`, `datetime`, `money`, …).
    pub sql_type: Option<String>,
    pub required: bool,
    pub auto_increment: bool,
    /// From `xsd:maxLength` on text columns.
    pub max_length: Option<u32>,
    /// `od:jetComplexType` for attachment/multi-valued columns.
    pub complex_type: Option<String>,
    /// `od:fieldProperty` entries: `(name, type code, value)`.
    pub properties: Vec<(String, String, String)>,
}

impl Column {
    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties.iter().find(|(n, _, _)| n == name).map(|(_, _, v)| v.as_str())
    }
    pub fn description(&self) -> Option<&str> {
        self.property("Description")
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub primary: bool,
    pub unique: bool,
    /// Per-column `asc`/`desc`.
    pub order: Vec<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    /// `od:tableProperty` entries: `(name, type code, value)`.
    pub properties: Vec<(String, String, String)>,
    /// Rows from the sample-data part, cells in `columns` order (`None` for absent/null).
    pub rows: Vec<Vec<Option<String>>>,
    /// Whether a sample-data part existed (a table can legitimately have zero rows).
    pub has_data_part: bool,
}

impl Table {
    pub fn primary_key(&self) -> Option<&Index> {
        self.indexes.iter().find(|i| i.primary)
    }

    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns.iter().find(|c| c.name == name)
    }

    /// RFC 4180 CSV with a header row; every value quoted.
    pub fn to_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = self.columns.iter().map(|c| q(&c.name)).collect::<Vec<_>>().join(",");
        out.push('\n');
        for row in &self.rows {
            let line: Vec<String> = row.iter().map(|v| v.as_deref().map(q).unwrap_or_default()).collect();
            out.push_str(&line.join(","));
            out.push('\n');
        }
        out
    }
}

pub(crate) fn parse_schema(part: &str, name: &str, bytes: &[u8]) -> crate::Result<Table> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    let table_el = doc
        .descendants()
        .filter(|n| n.has_tag_name((XSD, "element")))
        .find(|n| n.attribute("name").is_some_and(|a| a != "dataroot") && n.parent().is_some_and(|p| p.has_tag_name((XSD, "schema"))))
        .ok_or_else(|| crate::Error::Invalid { part: part.to_string(), reason: "no table element in schema".into() })?;
    let mut table = Table { name: name.to_string(), columns: Vec::new(), indexes: Vec::new(), properties: Vec::new(), rows: Vec::new(), has_data_part: false };
    for n in table_el.descendants() {
        if n.has_tag_name((OD, "index")) {
            let key = n.attribute("index-key").unwrap_or("");
            table.indexes.push(Index {
                name: n.attribute("index-name").unwrap_or("").to_string(),
                columns: key.split_whitespace().map(String::from).collect(),
                primary: n.attribute("primary") == Some("yes"),
                unique: n.attribute("unique") == Some("yes"),
                order: n.attribute("order").unwrap_or("").split_whitespace().map(String::from).collect(),
            });
        } else if n.has_tag_name((OD, "tableProperty")) {
            table.properties.push((
                n.attribute("name").unwrap_or("").to_string(),
                n.attribute("type").unwrap_or("").to_string(),
                n.attribute("value").unwrap_or("").trim().to_string(),
            ));
        }
    }
    let sequence = table_el.descendants().find(|n| n.has_tag_name((XSD, "sequence")));
    if let Some(seq) = sequence {
        for col in seq.children().filter(|n| n.has_tag_name((XSD, "element"))) {
            let mut properties = Vec::new();
            let mut max_length = None;
            for d in col.descendants() {
                if d.has_tag_name((OD, "fieldProperty")) {
                    properties.push((
                        d.attribute("name").unwrap_or("").to_string(),
                        d.attribute("type").unwrap_or("").to_string(),
                        d.attribute("value").unwrap_or("").trim().to_string(),
                    ));
                } else if d.has_tag_name((XSD, "maxLength")) {
                    max_length = d.attribute("value").and_then(|v| v.parse().ok());
                }
            }
            table.columns.push(Column {
                name: unescape_xml_name(col.attribute("name").unwrap_or("")),
                jet_type: col.attribute((OD, "jetType")).map(JetType::parse).unwrap_or(JetType::Other(String::new())),
                sql_type: col.attribute((OD, "sqlSType")).map(String::from),
                required: col.attribute((OD, "nonNullable")) == Some("yes"),
                auto_increment: col.attribute((OD, "autoUnique")) == Some("yes"),
                max_length,
                complex_type: col.attribute((OD, "jetComplexType")).map(String::from),
                properties,
            });
        }
    }
    Ok(table)
}

/// Fill `table.rows` from a sample-data part (`<root><xsd:schema/>…<dataroot><T>…</T></dataroot></root>`).
pub(crate) fn parse_data(part: &str, table: &mut Table, bytes: &[u8]) -> crate::Result<()> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    table.has_data_part = true;
    let Some(dataroot) = doc.descendants().find(|n| n.is_element() && n.tag_name().name() == "dataroot") else {
        return Ok(());
    };
    for row in dataroot.children().filter(|n| n.is_element()) {
        let mut cells: Vec<Option<String>> = vec![None; table.columns.len()];
        for cell in row.children().filter(|n| n.is_element()) {
            let name = unescape_xml_name(cell.tag_name().name());
            let value = cell.text().map(|t| t.to_string()).unwrap_or_default();
            match table.columns.iter().position(|c| c.name == name) {
                Some(i) => cells[i] = Some(value),
                None => {
                    table.columns.push(Column {
                        name,
                        jet_type: JetType::Other(String::new()),
                        sql_type: None,
                        required: false,
                        auto_increment: false,
                        max_length: None,
                        complex_type: None,
                        properties: Vec::new(),
                    });
                    for r in table.rows.iter_mut() {
                        r.push(None);
                    }
                    cells.push(Some(value));
                }
            }
        }
        table.rows.push(cells);
    }
    Ok(())
}
