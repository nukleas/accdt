//! Access Services (web database) objects, stored as AXL XML instead of SaveAsText:
//! forms (`axl:View` with XAML-style controls), reports (RDL-style) and queries.
//! Forms and reports are converted to the same [`Document`] tree SaveAsText produces, so
//! [`Design`](crate::Design) views work unchanged; queries become a [`QueryDef`].

use roxmltree::Node as XmlNode;

use crate::query::{Join, OutputColumn, QueryDef};
use crate::saveastext::{Document, Node};
use crate::text;

const SECTION_NAMES: &[&str] = &[
    "FormHeader",
    "FormFooter",
    "Detail",
    "PageHeader",
    "PageFooter",
    "ReportHeader",
    "ReportFooter",
    "GroupHeader",
    "GroupFooter",
];

fn children<'a, 'b>(n: XmlNode<'a, 'b>, tag: &str) -> Vec<XmlNode<'a, 'b>> {
    n.children()
        .filter(|c| c.is_element() && c.tag_name().name() == tag)
        .collect()
}

/// Attribute by local name, whatever its namespace prefix (`x:Name`, `Name`).
fn attr<'a>(n: XmlNode<'a, 'a>, name: &str) -> Option<&'a str> {
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}

fn set(node: &mut Node, key: &str, value: &str) {
    node.entries.push((key.to_string(), value.to_string()));
    node.properties
        .entry(key.to_string())
        .or_insert_with(|| value.to_string());
}

/// An `axl:View` form -> Document(Form).
pub(crate) fn form_document(part: &str, xml: &str) -> crate::Result<Document> {
    let doc = text::parse_xml(part, xml)?;
    let mut root = Node {
        kind: "Form".into(),
        ..Default::default()
    };
    let view = doc.root_element();
    if let Some(rs) = view
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "RecordSource")
        .and_then(|n| n.text())
    {
        set(&mut root, "RecordSource", rs.trim());
    }
    if let Some(form) = view
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "Form")
    {
        for a in form.attributes() {
            set(&mut root, a.name(), a.value());
        }
        let mut children = Vec::new();
        collect_controls(form, &mut children);
        root.children = children;
    }
    attach_macros(view, &mut root);
    let mut out = Document::default();
    out.header.insert("Format".into(), "axl".into());
    out.header_entries.push(("Format".into(), "axl".into()));
    out.blocks.push(root);
    Ok(out)
}

/// Named XAML elements become control nodes; unnamed layout elements (Grid, Border, Style) are transparent.
fn collect_controls(parent: XmlNode, out: &mut Vec<Node>) {
    for child in parent.children().filter(|c| c.is_element()) {
        let tag = child.tag_name().name();
        // `a:Form.Resources` holds styles; everything else (DataTemplate, Grid, Border) may wrap controls.
        if matches!(tag, "Style" | "Setter") || tag.ends_with(".Resources") {
            continue;
        }
        match attr(child, "Name") {
            Some(name) if !name.is_empty() => {
                let kind = if SECTION_NAMES.contains(&name) && tag == "Section" {
                    name.to_string()
                } else {
                    tag.to_string()
                };
                let mut node = Node {
                    kind,
                    ..Default::default()
                };
                set(&mut node, "Name", name);
                for a in child.attributes().filter(|a| a.name() != "Name") {
                    set(&mut node, a.name(), a.value());
                }
                let mut kids = Vec::new();
                collect_controls(child, &mut kids);
                node.children = kids;
                out.push(node);
            }
            _ => collect_controls(child, out),
        }
    }
}

/// `axl:UserInterfaceMacro For="ctl" Event="OnClick"` -> `OnClick = [Embedded Macro]` plus an
/// `OnClickEmMacro` child holding the flattened actions on the named control (or the form).
fn attach_macros(view: XmlNode, root: &mut Node) {
    for m in view
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "UserInterfaceMacro")
    {
        let target = attr(m, "For").unwrap_or("");
        let event = attr(m, "Event").unwrap_or("OnClick");
        let event =
            if event.starts_with("On") || event.starts_with("After") || event.starts_with("Before")
            {
                event.to_string()
            } else {
                format!("On{event}")
            };
        let mut em = Node {
            kind: format!("{event}EmMacro"),
            ..Default::default()
        };
        set(&mut em, "Version", "axl");
        flatten_actions(m, None, &mut em.children);
        let owner = if target.is_empty() {
            Some(&mut *root)
        } else {
            find_control(root, target)
        };
        if let Some(owner) = owner {
            set(owner, &event, "[Embedded Macro]");
            owner.children.push(em);
        }
    }
}

fn find_control<'a>(node: &'a mut Node, name: &str) -> Option<&'a mut Node> {
    if node.get("Name") == Some(name) {
        return Some(node);
    }
    node.children.iter_mut().find_map(|c| find_control(c, name))
}

/// Actions in document order; an `If`/`Else` condition is carried on each action it guards.
pub(crate) fn flatten_actions(n: XmlNode, condition: Option<&str>, out: &mut Vec<Node>) {
    for child in n.children().filter(|c| c.is_element()) {
        match child.tag_name().name() {
            "Action" => {
                let mut block = Node {
                    kind: "Block".into(),
                    ..Default::default()
                };
                if let Some(c) = condition {
                    set(&mut block, "Condition", c);
                }
                set(&mut block, "Action", attr(child, "Name").unwrap_or(""));
                for arg in child
                    .children()
                    .filter(|c| c.is_element() && c.tag_name().name() == "Argument")
                {
                    set(&mut block, "Argument", arg.text().unwrap_or("").trim());
                    set(&mut block, "ArgumentName", attr(arg, "Name").unwrap_or(""));
                }
                out.push(block);
            }
            "Comment" => {
                let mut block = Node {
                    kind: "Block".into(),
                    ..Default::default()
                };
                set(&mut block, "Comment", child.text().unwrap_or("").trim());
                out.push(block);
            }
            "If" | "ElseIf" => {
                let cond = child
                    .children()
                    .find(|c| c.is_element() && c.tag_name().name() == "Condition")
                    .and_then(|c| c.text())
                    .map(str::trim);
                flatten_actions(child, cond.or(condition), out);
            }
            "Else" => flatten_actions(child, Some("Else"), out),
            "Sub" => {
                let mut block = Node {
                    kind: "Block".into(),
                    ..Default::default()
                };
                set(
                    &mut block,
                    "Comment",
                    &format!("Submacro {}", attr(child, "Name").unwrap_or("")),
                );
                out.push(block);
                flatten_actions(child, condition, out);
            }
            _ => flatten_actions(child, condition, out),
        }
    }
}

/// An RDL-style `Report` -> Document(Report). Sections are the named top-level rectangles;
/// textboxes carry their value as `ControlSource` (expressions) or `Caption` (literals).
pub(crate) fn report_document(part: &str, xml: &str) -> crate::Result<Document> {
    let doc = text::parse_xml(part, xml)?;
    let mut root = Node {
        kind: "Report".into(),
        ..Default::default()
    };
    let report = doc.root_element();
    if let Some(cmd) = report
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "CommandText")
        .and_then(|n| n.text())
    {
        set(&mut root, "RecordSource", cmd.trim());
    }
    for group in report
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "GroupExpression")
    {
        if let Some(expr) = group.text() {
            set(&mut root, "GroupExpression", expr.trim());
        }
    }
    let mut children = Vec::new();
    collect_report_items(report, &mut children);
    root.children = children;
    let mut out = Document::default();
    out.header.insert("Format".into(), "axl".into());
    out.header_entries.push(("Format".into(), "axl".into()));
    out.blocks.push(root);
    Ok(out)
}

fn collect_report_items(parent: XmlNode, out: &mut Vec<Node>) {
    for child in parent.children().filter(|c| c.is_element()) {
        let tag = child.tag_name().name();
        match attr(child, "Name") {
            Some(name)
                if !name.is_empty()
                    && !matches!(tag, "Field" | "DataSet" | "DataSource" | "CustomProperty") =>
            {
                let kind = if SECTION_NAMES.contains(&name) && tag == "Rectangle" {
                    name.to_string()
                } else {
                    tag.to_string()
                };
                let mut node = Node {
                    kind,
                    ..Default::default()
                };
                set(&mut node, "Name", name);
                for prop in child.children().filter(|c| c.is_element()) {
                    let pname = prop.tag_name().name();
                    if let Some(t) = prop.text().filter(|t| !t.trim().is_empty()).filter(|_| {
                        matches!(
                            pname,
                            "Height"
                                | "Width"
                                | "Top"
                                | "Left"
                                | "DataSetName"
                                | "Hidden"
                                | "CanGrow"
                        )
                    }) {
                        set(&mut node, pname, t.trim());
                    }
                }
                if let Some(v) = child
                    .descendants()
                    .find(|d| d.is_element() && d.tag_name().name() == "Value")
                    .and_then(|d| d.text())
                {
                    let v = v.trim();
                    set(
                        &mut node,
                        if v.starts_with('=') {
                            "ControlSource"
                        } else {
                            "Caption"
                        },
                        v,
                    );
                }
                let mut kids = Vec::new();
                collect_report_items(child, &mut kids);
                node.children = kids;
                out.push(node);
            }
            _ => collect_report_items(child, out),
        }
    }
}

/// An AXL `Query` -> QueryDef (select). Expressions keep Access Services syntax.
pub(crate) fn query_def(part: &str, xml: &str) -> crate::Result<QueryDef> {
    let doc = text::parse_xml(part, xml)?;
    let q = doc.root_element();
    let mut def = QueryDef {
        operation: Some(crate::query::Operation::Select),
        ..Default::default()
    };
    let section = |tag: &str| {
        q.children()
            .find(|c| c.is_element() && c.tag_name().name() == tag)
    };
    if let Some(refs) = section("References") {
        for r in children(refs, "Reference") {
            def.tables.push((
                attr(r, "Source").unwrap_or("").to_string(),
                attr(r, "Alias").map(String::from),
            ));
        }
    }
    if let Some(joins) = section("Joins") {
        for j in children(joins, "Join") {
            def.joins.push(Join {
                left_table: attr(j, "LeftSource")
                    .or(attr(j, "Left"))
                    .unwrap_or("")
                    .to_string(),
                right_table: attr(j, "RightSource")
                    .or(attr(j, "Right"))
                    .unwrap_or("")
                    .to_string(),
                expression: j
                    .descendants()
                    .find(|d| d.is_element() && d.tag_name().name() == "Expression")
                    .and_then(|d| d.text())
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                flag: match attr(j, "Type").unwrap_or("Inner") {
                    "LeftOuter" => 2,
                    "RightOuter" => 3,
                    _ => 1,
                },
            });
        }
    }
    if let Some(results) = section("Results") {
        for p in children(results, "Property") {
            let source = attr(p, "Source");
            let expr = p
                .children()
                .find(|c| c.is_element() && c.tag_name().name() == "Expression")
                .and_then(|c| c.text())
                .map(|t| t.trim().to_string());
            let expression = match (attr(p, "All"), attr(p, "Name"), source, expr) {
                (Some("true"), _, Some(s), _) => format!("{s}.*"),
                (_, _, _, Some(e)) => e,
                (_, Some(n), Some(s), _) => format!("{s}.{n}"),
                (_, Some(n), None, _) => n.to_string(),
                _ => continue,
            };
            def.columns.push(OutputColumn {
                expression,
                alias: attr(p, "Alias").map(String::from),
            });
        }
    }
    if let Some(r) = section("Restriction") {
        def.where_clause = r
            .descendants()
            .find(|d| d.is_element() && d.tag_name().name() == "Expression")
            .and_then(|d| d.text())
            .map(|t| t.trim().to_string());
    }
    if let Some(o) = section("Ordering") {
        for ord in children(o, "Order") {
            let expr = match (attr(ord, "Source"), attr(ord, "Name")) {
                (Some(s), Some(n)) => format!("{s}.{n}"),
                (_, Some(n)) => n.to_string(),
                _ => continue,
            };
            def.order_by.push((
                expr,
                attr(ord, "Direction").is_some_and(|d| d.eq_ignore_ascii_case("Descending")),
            ));
        }
    }
    if let Some(g) = section("Grouping") {
        for grp in children(g, "Group") {
            if let (Some(s), Some(n)) = (attr(grp, "Source"), attr(grp, "Name")) {
                def.group_by.push(format!("{s}.{n}"));
            }
        }
    }
    def.properties.push(("Format".into(), "axl".into()));
    if let Some(name) = attr(q, "Name") {
        def.properties.push(("Name".into(), name.to_string()));
    }
    Ok(def)
}

/// A SharePoint list definition (`.caml` part): the SOAP calls Access replays to create the list.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ListDefinition {
    pub part: String,
    pub list_name: String,
    /// SharePoint list template id (100 generic list, 105 contacts, 107 tasks, 1100 issues, …).
    pub template_id: Option<u32>,
    pub fields: Vec<ListField>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ListField {
    pub display_name: String,
    /// SharePoint field type (`Text`, `Note`, `DateTime`, `Lookup`, `Number`, …).
    pub field_type: String,
    pub internal_name: Option<String>,
    pub required: bool,
    /// Whether the SOAP method adds (`true`) or updates (`false`) the field.
    pub added: bool,
}

pub(crate) fn list_definition(part: &str, bytes: &[u8]) -> crate::Result<ListDefinition> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    let root = doc.root_element();
    let find_text = |tag: &str| {
        root.descendants()
            .find(|n| n.is_element() && n.tag_name().name() == tag)
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
    };
    let mut def = ListDefinition {
        part: part.to_string(),
        list_name: root
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "AddList")
            .and_then(|a| {
                a.children()
                    .find(|c| c.is_element() && c.tag_name().name() == "listName")
            })
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
            .or_else(|| find_text("listName"))
            .unwrap_or_default(),
        template_id: find_text("templateID").and_then(|t| t.parse().ok()),
        fields: Vec::new(),
    };
    for group in root
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "newFields" | "updateFields"))
    {
        let added = group.tag_name().name() == "newFields";
        for f in group
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "Field")
        {
            def.fields.push(ListField {
                display_name: attr(f, "DisplayName").unwrap_or("").to_string(),
                field_type: attr(f, "Type").unwrap_or("").to_string(),
                internal_name: attr(f, "Name").or(attr(f, "StaticName")).map(String::from),
                required: attr(f, "Required").is_some_and(|r| r.eq_ignore_ascii_case("TRUE")),
                added,
            });
        }
    }
    Ok(def)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::design::{Design, DesignKind};

    const FORM: &str = r#"<axl:View Name="Contacts" xmlns="http://schemas.microsoft.com/client/2009/11" xmlns:a="http://schemas.microsoft.com/office/accessservices/2009/11/forms" xmlns:axl="http://schemas.microsoft.com/office/accessservices/2009/11/application" xmlns:x="http://schemas.microsoft.com/winfx/2009/04/xaml"><axl:UserInterfaceMacros><axl:UserInterfaceMacro For="cmdSearch" Event="OnClick"><axl:Statements><axl:ConditionalBlock><axl:If><axl:Condition>=txtSearch="Search..."</axl:Condition><axl:Statements><axl:Action Name="SetFilter"><axl:Argument Name="WhereCondition">=SQL.Like(Searchable,"*")</axl:Argument></axl:Action></axl:Statements></axl:If></axl:ConditionalBlock></axl:Statements></axl:UserInterfaceMacro></axl:UserInterfaceMacros><axl:Data><axl:RecordSource>Contacts</axl:RecordSource></axl:Data><a:Form Caption="Contact List" PageSize="20"><Grid><a:Section x:Name="Detail"><Border><a:TextBox x:Name="txtContactName" ControlSource="ContactName" ControlWidth="165" /></Border><a:Button x:Name="cmdSearch" Caption="Go" /></a:Section></Grid></a:Form></axl:View>"#;

    #[test]
    fn form_becomes_a_design() {
        let doc = form_document("f", FORM).unwrap();
        let d = Design {
            name: "Contacts".into(),
            kind: DesignKind::Form,
            document: doc,
            text: FORM.into(),
        };
        assert_eq!(d.record_source(), Some("Contacts"));
        assert_eq!(d.get("Caption"), Some("Contact List"));
        let ctrls = d.controls();
        let names: Vec<&str> = ctrls.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Detail", "txtContactName", "cmdSearch"]);
        assert_eq!(ctrls[1].control_type, "TextBox");
        assert_eq!(ctrls[1].section, "Detail");
        assert_eq!(ctrls[1].get("ControlSource"), Some("ContactName"));
        let events = d.events();
        assert!(
            events.iter().any(|e| e.owner == "cmdSearch"
                && e.event == "Click"
                && e.value == "[Embedded Macro]"),
            "{events:?}"
        );
        let m = &d.embedded_macros()["cmdSearch.Click"];
        assert_eq!(m.actions[0].action, "SetFilter");
        assert_eq!(
            m.actions[0].condition.as_deref(),
            Some("=txtSearch=\"Search...\"")
        );
    }

    #[test]
    fn query_becomes_a_def() {
        let xml = r#"<Query Name="ContactsExtended" xmlns="x"><References><Reference Source="Contacts"/></References><Results><Property Source="Contacts" All="true"/><Property Alias="Searchable"><Expression>=Concatenate.Db(LastName,FirstName)</Expression></Property></Results><Ordering><Order Source="Contacts" Name="ContactName" Direction="Descending"/></Ordering></Query>"#;
        let def = query_def("q", xml).unwrap();
        let sql = def.to_sql();
        assert_eq!(
            sql.text,
            "SELECT Contacts.*, =Concatenate.Db(LastName,FirstName) AS Searchable\nFROM Contacts\nORDER BY Contacts.ContactName DESC\n;"
        );
    }

    #[test]
    fn list_definition_fields() {
        let xml = br#"<Root><SoapMethod>AddList</SoapMethod><Envelope><Body><AddList><listName>Comments</listName><templateID>100</templateID></AddList></Body></Envelope><Envelope><Body><UpdateList><newFields><Fields><Method ID="1"><Field DisplayName="CommentDate" Type="DateTime"/></Method></Fields></newFields><updateFields><Fields><Method ID="2"><Field Type="Text" Name="Title" DisplayName="SharePointTitle" Required="FALSE"/></Method></Fields></updateFields></UpdateList></Body></Envelope></Root>"#;
        let d = list_definition("t.caml", xml).unwrap();
        assert_eq!(d.list_name, "Comments");
        assert_eq!(d.template_id, Some(100));
        assert_eq!(d.fields.len(), 2);
        assert!(d.fields[0].added && d.fields[0].field_type == "DateTime");
        assert_eq!(d.fields[1].internal_name.as_deref(), Some("Title"));
    }
}
