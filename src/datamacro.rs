//! Table data macros (`dataMacros/*.axl`): event-driven logic attached to tables.

use crate::text;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DataMacroAction {
    pub name: String,
    /// `(argument name, value)`.
    pub arguments: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DataMacro {
    /// `BeforeChange`, `AfterInsert`, … or the name of a named data macro.
    pub event: String,
    /// Actions in document order (nested blocks flattened).
    pub actions: Vec<DataMacroAction>,
    /// The macro's XML, for the full structure (conditions, loops, comments).
    pub xml: String,
}

pub(crate) fn parse(part: &str, bytes: &[u8]) -> crate::Result<Vec<DataMacro>> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    let mut out = Vec::new();
    for dm in doc
        .root_element()
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "DataMacro")
    {
        let event = dm
            .attribute("Event")
            .or_else(|| dm.attribute("Name"))
            .unwrap_or("")
            .to_string();
        let actions = dm
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "Action")
            .map(|a| DataMacroAction {
                name: a.attribute("Name").unwrap_or("").to_string(),
                arguments: a
                    .children()
                    .filter(|c| c.is_element() && c.tag_name().name() == "Argument")
                    .map(|c| {
                        (
                            c.attribute("Name").unwrap_or("").to_string(),
                            c.text().unwrap_or("").trim().to_string(),
                        )
                    })
                    .collect(),
            })
            .collect();
        let range = dm.range();
        out.push(DataMacro {
            event,
            actions,
            xml: xml[range].to_string(),
        });
    }
    Ok(out)
}
