//! Database-level parts: template metadata, core properties, database and object
//! properties, relationships, VBA references.

use crate::text;

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TemplateInfo {
    pub template_format: Option<String>,
    pub required_access_version: Option<String>,
    pub access_services_version: Option<String>,
    pub template_type: Option<String>,
    pub data_locale: Option<String>,
    pub ui_locale: Option<String>,
    pub collating_order: Option<String>,
    pub flip_right_to_left: bool,
    pub perform_localization_fixup: bool,
    pub perform_font_fixup: bool,
    /// Which localisation variation the template was saved as (`""` for the base).
    pub variation_identifier: Option<String>,
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CoreProperties {
    pub title: Option<String>,
    pub description: Option<String>,
    pub creator: Option<String>,
    pub category: Option<String>,
    pub keywords: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
}

/// A typed property (`Type` is Access's DAO type code: 1 Boolean, 2 Byte, 3 Integer,
/// 4 Long, 5 Currency, 6 Single, 7 Double, 8 Date, 9 Binary (base64 here), 10 Text,
/// 11 LongBinary, 12 Memo).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Property {
    pub name: String,
    pub type_code: Option<u32>,
    pub value: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Relationship {
    pub name: String,
    /// Child (referencing) table.
    pub table: String,
    /// Parent (referenced) table.
    pub referenced_table: String,
    /// `(column, referenced column)` pairs.
    pub columns: Vec<(String, String)>,
    /// Raw `grbit` flags.
    pub flags: u32,
}

impl Relationship {
    pub fn enforces_integrity(&self) -> bool {
        self.flags & 0x2 == 0
    }
    pub fn cascade_update(&self) -> bool {
        self.flags & 0x100 != 0
    }
    pub fn cascade_delete(&self) -> bool {
        self.flags & 0x1000 != 0
    }
    pub fn one_to_one(&self) -> bool {
        self.flags & 0x1 != 0
    }
    /// Relationship window shows a left outer join from the child table.
    pub fn left_join(&self) -> bool {
        self.flags & 0x0100_0000 != 0
    }
    pub fn right_join(&self) -> bool {
        self.flags & 0x0200_0000 != 0
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct VbaReference {
    pub guid: String,
    pub major: u32,
    pub minor: u32,
}

impl VbaReference {
    /// Library name for well-known type libraries.
    pub fn known_name(&self) -> Option<&'static str> {
        KNOWN
            .iter()
            .find(|(g, _, _)| g.eq_ignore_ascii_case(&self.guid))
            .map(|(_, n, _)| *n)
    }

    /// True for libraries that only ever shipped as 32-bit (they cannot load in 64-bit Office).
    pub fn is_32bit_only(&self) -> bool {
        KNOWN
            .iter()
            .find(|(g, _, _)| g.eq_ignore_ascii_case(&self.guid))
            .is_some_and(|(_, _, b)| *b)
    }
}

const KNOWN: &[(&str, &str, bool)] = &[
    (
        "{000204EF-0000-0000-C000-000000000046}",
        "Visual Basic For Applications",
        false,
    ),
    (
        "{4AFFC9A0-5F99-101B-AF4E-00AA003F0F07}",
        "Microsoft Access Object Library",
        false,
    ),
    (
        "{00020430-0000-0000-C000-000000000046}",
        "OLE Automation (stdole)",
        false,
    ),
    (
        "{4AC9E1DA-5BAD-4AC7-86E3-24F4CDCECA28}",
        "Microsoft Office Access database engine Object Library (ACE DAO)",
        false,
    ),
    (
        "{00025E01-0000-0000-C000-000000000046}",
        "Microsoft DAO 3.6 Object Library",
        true,
    ),
    (
        "{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}",
        "Microsoft Office Object Library",
        false,
    ),
    (
        "{00062FFF-0000-0000-C000-000000000046}",
        "Microsoft Outlook Object Library",
        false,
    ),
    (
        "{00020813-0000-0000-C000-000000000046}",
        "Microsoft Excel Object Library",
        false,
    ),
    (
        "{00020905-0000-0000-C000-000000000046}",
        "Microsoft Word Object Library",
        false,
    ),
    (
        "{420B2830-E718-11CF-893D-00A0C9054228}",
        "Microsoft Scripting Runtime",
        false,
    ),
    (
        "{B691E011-1797-432E-907A-4D8C69339129}",
        "Microsoft ActiveX Data Objects 6.1 Library",
        false,
    ),
    (
        "{2A75196C-D9EB-4129-B803-931327F72D5C}",
        "Microsoft ActiveX Data Objects 2.8 Library",
        false,
    ),
    (
        "{0D452EE1-E08F-101A-852E-02608C4D0BB4}",
        "Microsoft Forms 2.0 Object Library",
        false,
    ),
    (
        "{831FDD16-0C5C-11D2-A9FC-0000F8754DA1}",
        "Microsoft Windows Common Controls 6.0 (MSCOMCTL.OCX)",
        true,
    ),
    (
        "{86CF1D34-0C5F-11D2-A9FC-0000F8754DA1}",
        "Microsoft Windows Common Controls-2 6.0 (MSCOMCT2.OCX)",
        true,
    ),
    (
        "{8E27C92E-1264-101C-8A2F-040224009C02}",
        "Microsoft Calendar Control (MSCAL.OCX)",
        true,
    ),
    (
        "{5E9E78A0-531B-11CF-91F6-C2863C385E30}",
        "Microsoft FlexGrid Control 6.0 (MSFLXGRD.OCX)",
        true,
    ),
    (
        "{648A5603-2C6E-101B-82B6-000000000014}",
        "Microsoft Common Dialog Control 6.0 (COMDLG32.OCX)",
        true,
    ),
    (
        "{F5078F18-C551-11D3-89B9-0000F81FE221}",
        "Microsoft XML, v6.0",
        false,
    ),
    (
        "{3F4DACA7-160D-11D2-A8E9-00104B365C9F}",
        "Microsoft VBScript Regular Expressions 5.5",
        false,
    ),
];

fn child_text<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<String> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
        .and_then(|c| c.text())
        .map(|t| t.trim().to_string())
}

pub(crate) fn parse_template(part: &str, bytes: &[u8]) -> crate::Result<TemplateInfo> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    let root = doc.root_element();
    Ok(TemplateInfo {
        template_format: child_text(root, "TemplateFormat"),
        required_access_version: child_text(root, "RequiredAccessVersion"),
        access_services_version: child_text(root, "AccessServicesVersion"),
        template_type: child_text(root, "Type"),
        data_locale: child_text(root, "DataLocale"),
        ui_locale: child_text(root, "UILocale"),
        collating_order: child_text(root, "CollatingOrder"),
        flip_right_to_left: child_text(root, "FlipRightToLeft").is_some_and(|v| v == "1"),
        perform_localization_fixup: child_text(root, "PerformLocalizationFixup")
            .is_some_and(|v| v == "1"),
        perform_font_fixup: child_text(root, "PerformFontFixup").is_some_and(|v| v == "1"),
        variation_identifier: child_text(root, "VariationIdentifier")
            .map(|v| v.trim_matches('"').to_string()),
    })
}

pub(crate) fn parse_core(part: &str, bytes: &[u8]) -> crate::Result<CoreProperties> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    let root = doc.root_element();
    Ok(CoreProperties {
        title: child_text(root, "title"),
        description: child_text(root, "description"),
        creator: child_text(root, "creator"),
        category: child_text(root, "category"),
        keywords: child_text(root, "keywords"),
        created: child_text(root, "created"),
        modified: child_text(root, "modified"),
    })
}

/// `<Properties><Property Name=".." Type="..">value</Property>…` (database and object properties).
pub(crate) fn parse_properties(part: &str, bytes: &[u8]) -> crate::Result<Vec<Property>> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    Ok(doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "Property")
        .map(|n| Property {
            name: n.attribute("Name").unwrap_or("").to_string(),
            type_code: n.attribute("Type").and_then(|t| t.parse().ok()),
            value: n.text().unwrap_or("").trim().to_string(),
        })
        .collect())
}

pub(crate) fn parse_relationships(part: &str, bytes: &[u8]) -> crate::Result<Vec<Relationship>> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    let mut out: Vec<Relationship> = Vec::new();
    for row in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "MSysRelationships")
    {
        let get = |k: &str| child_text(row, k).unwrap_or_default();
        let name = get("szRelationship");
        let pair = (get("szColumn"), get("szReferencedColumn"));
        let position: usize = get("icolumn").parse().unwrap_or(0);
        if position >= 255 {
            return Err(crate::Error::Invalid {
                part: part.to_string(),
                reason: format!("relationship {name} has column position {position}"),
            });
        }
        match out.iter_mut().find(|r| r.name == name) {
            Some(r) => {
                if position >= r.columns.len() {
                    r.columns
                        .resize(position + 1, (String::new(), String::new()));
                }
                r.columns[position] = pair;
            }
            None => {
                let mut columns = vec![(String::new(), String::new()); position + 1];
                columns[position] = pair;
                out.push(Relationship {
                    name,
                    table: get("szObject"),
                    referenced_table: get("szReferencedObject"),
                    columns,
                    flags: get("grbit").parse().unwrap_or(0),
                });
            }
        }
    }
    Ok(out)
}

pub(crate) fn parse_references(part: &str, bytes: &[u8]) -> crate::Result<Vec<VbaReference>> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    Ok(doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "VBAReference")
        .map(|n| VbaReference {
            guid: child_text(n, "GUID").unwrap_or_default(),
            major: child_text(n, "MajorVer")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            minor: child_text(n, "MinorVer")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
        })
        .collect())
}
