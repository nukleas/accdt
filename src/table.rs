//! Tables: schema from the object's XML Schema part, rows from its sample-data part.

use std::collections::BTreeMap;

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
        self.properties
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, _, v)| v.as_str())
    }
    pub fn display_control(&self) -> crate::PropertyResult<crate::ControlKind> {
        self.property("DisplayControl")
            .map(|value| {
                crate::design::integer(
                    "",
                    &self.name,
                    "DisplayControl",
                    crate::Resolved {
                        value,
                        origin: crate::PropertyOrigin::Explicit,
                    },
                    crate::ControlKind::from_code,
                )
            })
            .transpose()
    }
    pub fn format(&self) -> Option<crate::DisplayFormat<'_>> {
        self.property("Format").map(crate::DisplayFormat::parse)
    }
    pub fn input_mask(&self) -> Option<&str> {
        self.property("InputMask")
    }
    pub fn decimal_places(&self) -> crate::PropertyResult<crate::DecimalPlaces> {
        self.property("DecimalPlaces")
            .map(|value| {
                crate::design::integer(
                    "",
                    &self.name,
                    "DecimalPlaces",
                    crate::Resolved {
                        value,
                        origin: crate::PropertyOrigin::Explicit,
                    },
                    crate::DecimalPlaces::from_code,
                )
            })
            .transpose()
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

/// A cell from the sample data.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Value {
    Text(String),
    /// Attachment or multi-valued field: one record per child element, each a map of its
    /// child elements (`FileName`, `FileData` as base64, `FileType`, or `Value`).
    Complex(Vec<BTreeMap<String, String>>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            Value::Complex(_) => None,
        }
    }

    /// Text for CSV: scalars verbatim; complex values as `key=value` pairs joined with `; `,
    /// records separated by ` | `, binary payloads summarised by size.
    pub fn to_csv_text(&self) -> String {
        match self {
            Value::Text(s) => s.clone(),
            Value::Complex(records) => records
                .iter()
                .map(|r| {
                    r.iter()
                        .map(|(k, v)| {
                            if k == "FileData" {
                                format!("{k}=<{} base64 chars>", v.len())
                            } else {
                                format!("{k}={v}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }
}

/// A table that is a link to a SharePoint list rather than local data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SharePointList {
    /// List template id (100 generic, 105 contacts, 106 events, 107 tasks, 1100 issues, …).
    pub template_id: Option<u32>,
    pub root_folder: Option<String>,
    pub default_view_url: Option<String>,
    pub version: Option<String>,
    pub last_modified: Option<String>,
    pub display_views_on_site: bool,
    pub document_library: bool,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    /// `od:tableProperty` entries: `(name, type code, value)`.
    pub properties: Vec<(String, String, String)>,
    /// Present when the table is a SharePoint list link (`WSS*` table properties).
    pub sharepoint: Option<SharePointList>,
    /// Rows from the sample-data part, cells in `columns` order (`None` for absent/null).
    pub rows: Vec<Vec<Option<Value>>>,
    /// Whether a sample-data part existed (a table can legitimately have zero rows).
    pub has_data_part: bool,
}

impl Table {
    pub fn primary_key(&self) -> Option<&Index> {
        self.indexes.iter().find(|i| i.primary)
    }

    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, _, v)| v.as_str())
    }

    /// True for a SharePoint list link whose rows live on the server. A table that carries
    /// `WSS*` properties *and* a data part is local data that Access can publish to a list
    /// of that template id when the database is created on a site (the 2007 desktop
    /// templates ship this way); it is not linked.
    pub fn is_linked(&self) -> bool {
        self.sharepoint.is_some() && !self.has_data_part
    }

    /// Column by name, case-insensitively (Access names are case-insensitive).
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// RFC 4180 CSV with a header row; every value quoted.
    pub fn to_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = self
            .columns
            .iter()
            .map(|c| q(&c.name))
            .collect::<Vec<_>>()
            .join(",");
        out.push('\n');
        for row in &self.rows {
            let line: Vec<String> = row
                .iter()
                .map(|v| v.as_ref().map(|v| q(&v.to_csv_text())).unwrap_or_default())
                .collect();
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
        .find(|n| {
            n.attribute("name").is_some_and(|a| a != "dataroot")
                && n.parent().is_some_and(|p| p.has_tag_name((XSD, "schema")))
        })
        .ok_or_else(|| crate::Error::Invalid {
            part: part.to_string(),
            reason: "no table element in schema".into(),
        })?;
    let mut table = Table {
        name: name.to_string(),
        columns: Vec::new(),
        indexes: Vec::new(),
        properties: Vec::new(),
        sharepoint: None,
        rows: Vec::new(),
        has_data_part: false,
    };
    for n in table_el.descendants() {
        if n.has_tag_name((OD, "index")) {
            let key = n.attribute("index-key").unwrap_or("");
            table.indexes.push(Index {
                name: n.attribute("index-name").unwrap_or("").to_string(),
                columns: key.split_whitespace().map(unescape_xml_name).collect(),
                primary: n.attribute("primary") == Some("yes"),
                unique: n.attribute("unique") == Some("yes"),
                order: n
                    .attribute("order")
                    .unwrap_or("")
                    .split_whitespace()
                    .map(String::from)
                    .collect(),
            });
        } else if n.has_tag_name((OD, "tableProperty")) {
            table.properties.push((
                n.attribute("name").unwrap_or("").to_string(),
                n.attribute("type").unwrap_or("").to_string(),
                n.attribute("value").unwrap_or("").trim().to_string(),
            ));
        }
    }
    if table
        .properties
        .iter()
        .any(|(n, _, _)| n.starts_with("WSS"))
    {
        table.sharepoint = Some(SharePointList {
            template_id: table.property("WSSTemplateID").and_then(|v| v.parse().ok()),
            root_folder: table.property("WSSRootFolder").map(String::from),
            default_view_url: table.property("DefaultViewUrl").map(String::from),
            version: table.property("WSSVersion").map(String::from),
            last_modified: table.property("WSSLastModified").map(String::from),
            display_views_on_site: table.property("DisplayViewsOnSharePointSite") == Some("1"),
            document_library: table.property("DocumentLibrary") == Some("1"),
        });
    }
    let sequence = table_el
        .descendants()
        .find(|n| n.has_tag_name((XSD, "sequence")));
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
                jet_type: col
                    .attribute((OD, "jetType"))
                    .map(JetType::parse)
                    .unwrap_or(JetType::Other(String::new())),
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
/// A cell with element children (attachments, multi-valued fields) becomes a
/// [`Value::Complex`] record; repeated cells with the same name accumulate records.
pub(crate) fn parse_data(part: &str, table: &mut Table, bytes: &[u8]) -> crate::Result<()> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    table.has_data_part = true;
    let Some(dataroot) = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "dataroot")
    else {
        return Ok(());
    };
    for row in dataroot.children().filter(|n| n.is_element()) {
        let mut cells: Vec<Option<Value>> = vec![None; table.columns.len()];
        for cell in row.children().filter(|n| n.is_element()) {
            let name = unescape_xml_name(cell.tag_name().name());
            let index = match table.columns.iter().position(|c| c.name == name) {
                Some(i) => i,
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
                    cells.push(None);
                    table.columns.len() - 1
                }
            };
            let has_children = cell.children().any(|c| c.is_element());
            if has_children {
                let record: BTreeMap<String, String> = cell
                    .children()
                    .filter(|c| c.is_element())
                    .map(|c| {
                        (
                            unescape_xml_name(c.tag_name().name()),
                            c.text()
                                .map(|t| t.split_whitespace().collect::<String>())
                                .unwrap_or_default(),
                        )
                    })
                    .collect();
                match &mut cells[index] {
                    Some(Value::Complex(records)) => records.push(record),
                    slot => *slot = Some(Value::Complex(vec![record])),
                }
            } else {
                cells[index] = Some(Value::Text(cell.text().unwrap_or("").to_string()));
            }
        }
        table.rows.push(cells);
    }
    Ok(())
}
