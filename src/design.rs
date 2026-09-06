//! Forms and reports: the SaveAsText design plus derived views (controls, events,
//! embedded macros, code-behind).

use std::collections::BTreeMap;

use crate::saveastext::{self, Document, Node};
use crate::text;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DesignKind {
    Form,
    Report,
}

const SECTION_KINDS: &[&str] = &[
    "FormHeader", "FormFooter", "PageHeader", "PageFooter", "ReportHeader", "ReportFooter", "GroupHeader", "GroupFooter", "Section", "Detail",
];

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Control {
    pub name: String,
    /// `TextBox`, `CommandButton`, `Subform`, `FormHeader`, `Section`, …
    pub control_type: String,
    /// Name of the section that contains it (sections name themselves).
    pub section: String,
    /// Containing control for nested controls (tab pages, option groups), or the section.
    pub parent: Option<String>,
    pub properties: BTreeMap<String, String>,
}

impl Control {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }
    pub fn is_section(&self) -> bool {
        SECTION_KINDS.contains(&self.control_type.as_str())
    }
    /// Layout in twips (1440 per inch) as written on the control. SaveAsText omits values
    /// equal to the design's per-type defaults; use [`Design::layout`] to fill those in.
    pub fn layout(&self) -> Layout {
        let n = |k: &str| self.get(k).and_then(|v| v.parse().ok());
        Layout { left: n("Left"), top: n("Top"), width: n("Width"), height: n("Height") }
    }
}

/// Position and size in twips (1440 per inch).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Layout {
    pub left: Option<i64>,
    pub top: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

/// An event property whose value is an event procedure, an embedded macro, or an expression.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Event {
    /// Control name, or `Form` / `Report` for the object itself.
    pub owner: String,
    /// `Click`, `Load`, `AfterUpdate`, … (the property name without `On`).
    pub event: String,
    /// `[Event Procedure]`, `[Embedded Macro]`, a macro name, or an `=expression`.
    pub value: String,
}

impl Event {
    /// The VBA procedure name Access binds for `[Event Procedure]`.
    pub fn procedure_name(&self) -> Option<String> {
        (self.value == "[Event Procedure]").then(|| format!("{}_{}", self.owner.replace(' ', "_"), self.event))
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Design {
    pub name: String,
    pub kind: DesignKind,
    /// Header lines (`Version`, `Checksum`, …).
    pub header: BTreeMap<String, String>,
    /// Form/report level properties (`RecordSource`, `Caption`, `DefaultView`, …).
    pub properties: BTreeMap<String, String>,
    /// Per-control-type default properties from the design's anonymous defaults block.
    pub control_defaults: BTreeMap<String, BTreeMap<String, String>>,
    /// The raw parsed document, for anything the typed views do not expose.
    pub document: Document,
    /// VBA source of the code-behind module, if the object has one.
    pub code_behind: Option<String>,
    /// The original SaveAsText text.
    pub text: String,
}

impl Design {
    /// Parse a form or report from SaveAsText text (a template part, or `Application.SaveAsText` output).
    pub fn parse(name: &str, kind: DesignKind, text: String) -> Design {
        let document = saveastext::parse(&text);
        let root = document.blocks.first().cloned().unwrap_or_default();
        let mut control_defaults = BTreeMap::new();
        for block in root.children.iter().filter(|c| c.kind == "Block") {
            for d in &block.children {
                if d.get("Name").is_none() && !SECTION_KINDS.contains(&d.kind.as_str()) {
                    control_defaults.insert(d.kind.clone(), d.properties.clone());
                }
            }
        }
        Design {
            name: name.to_string(),
            kind,
            header: document.header.clone(),
            properties: root.properties.clone(),
            control_defaults,
            code_behind: document.code_behind.clone().filter(|c| !c.trim().is_empty()),
            document,
            text,
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    pub fn record_source(&self) -> Option<&str> {
        self.get("RecordSource")
    }

    /// A control's layout with missing values filled from the design's defaults for its type.
    pub fn layout(&self, control: &Control) -> Layout {
        let mut l = control.layout();
        if let Some(d) = self.control_defaults.get(&control.control_type) {
            let n = |k: &str| d.get(k).and_then(|v| v.parse().ok());
            l.left = l.left.or_else(|| n("Left"));
            l.top = l.top.or_else(|| n("Top"));
            l.width = l.width.or_else(|| n("Width"));
            l.height = l.height.or_else(|| n("Height"));
        }
        l
    }

    /// Sections and controls in design order, sections first within their subtree.
    pub fn controls(&self) -> Vec<Control> {
        let mut out = Vec::new();
        if let Some(root) = self.document.blocks.first() {
            for child in &root.children {
                walk(child, "", None, &mut out);
            }
        }
        out
    }

    /// Every `On*` property with a value, on the object and its controls.
    pub fn events(&self) -> Vec<Event> {
        let owner = match self.kind {
            DesignKind::Form => "Form",
            DesignKind::Report => "Report",
        };
        let mut out = Vec::new();
        collect_events(owner, &self.properties, &mut out);
        for c in self.controls() {
            collect_events(&c.name, &c.properties, &mut out);
        }
        out
    }

    /// Embedded macros, decoded from their hex `*EmMacro` properties, keyed `owner.Event`.
    pub fn embedded_macros(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        let owner = match self.kind {
            DesignKind::Form => "Form",
            DesignKind::Report => "Report",
        };
        let mut grab = |who: &str, props: &BTreeMap<String, String>| {
            for (k, v) in props {
                if let (Some(ev), Some(bytes)) = (k.strip_suffix("EmMacro"), saveastext::hex_bytes(v)) {
                    out.insert(format!("{who}.{}", ev.trim_start_matches("On")), text::decode(&bytes));
                }
            }
        };
        grab(owner, &self.properties);
        for c in self.controls() {
            grab(&c.name, &c.properties);
        }
        out
    }
}

fn collect_events(owner: &str, props: &BTreeMap<String, String>, out: &mut Vec<Event>) {
    for (k, v) in props {
        let Some(ev) = k.strip_prefix("On") else { continue };
        if !k.ends_with("EmMacro") && !v.is_empty() && ev.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
            out.push(Event { owner: owner.to_string(), event: ev.to_string(), value: v.clone() });
        }
    }
}

fn walk(node: &Node, section: &str, parent: Option<&str>, out: &mut Vec<Control>) {
    if node.kind == "Block" {
        for child in &node.children {
            walk(child, section, parent, out);
        }
        return;
    }
    let Some(name) = node.get("Name").map(String::from) else { return };
    let is_section = SECTION_KINDS.contains(&node.kind.as_str());
    let section_name = if is_section { name.clone() } else { section.to_string() };
    out.push(Control {
        name: name.clone(),
        control_type: node.kind.clone(),
        section: section_name.clone(),
        parent: if is_section { None } else { parent.map(String::from) },
        properties: node.properties.clone(),
    });
    for child in &node.children {
        walk(child, &section_name, Some(&name), out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORM: &str = "Version =21\nBegin Form\n    RecordSource =\"Customers\"\n    OnLoad =\"[Event Procedure]\"\n    Begin\n        Begin Label\n            FontSize =11\n        End\n        Begin FormHeader\n            Name =\"FormHeader\"\n            Begin\n                Begin Label\n                    Name =\"lblTitle\"\n                    Caption =\"Customers\"\n                End\n            End\n        End\n        Begin Section\n            Name =\"Detail\"\n            Begin\n                Begin Tab\n                    Name =\"tabMain\"\n                    Begin\n                        Begin Page\n                            Name =\"pgOne\"\n                            Begin\n                                Begin CommandButton\n                                    Name =\"cmdSave\"\n                                    Left =100\n                                    Top =200\n                                    Width =1000\n                                    Height =300\n                                    OnClick =\"[Event Procedure]\"\n                                    OnDblClick =\"[Embedded Macro]\"\n                                End\n                            End\n                        End\n                    End\n                End\n            End\n        End\n    End\nEnd\nCodeBehindForm\nOption Compare Database\nPrivate Sub cmdSave_Click()\nEnd Sub\n";

    #[test]
    fn controls_events_and_code() {
        let d = Design::parse("frmCustomers", DesignKind::Form, FORM.to_string());
        assert_eq!(d.record_source(), Some("Customers"));
        assert_eq!(d.control_defaults["Label"]["FontSize"], "11");
        let ctrls = d.controls();
        let names: Vec<&str> = ctrls.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["FormHeader", "lblTitle", "Detail", "tabMain", "pgOne", "cmdSave"]);
        let save = ctrls.iter().find(|c| c.name == "cmdSave").unwrap();
        assert_eq!(save.section, "Detail");
        assert_eq!(save.parent.as_deref(), Some("pgOne"));
        assert_eq!(save.layout(), Layout { left: Some(100), top: Some(200), width: Some(1000), height: Some(300) });
        let events = d.events();
        assert!(events.iter().any(|e| e.owner == "Form" && e.event == "Load"));
        let click = events.iter().find(|e| e.owner == "cmdSave" && e.event == "Click").unwrap();
        assert_eq!(click.procedure_name().as_deref(), Some("cmdSave_Click"));
        assert!(events.iter().any(|e| e.event == "DblClick" && e.value == "[Embedded Macro]"));
        assert!(d.code_behind.unwrap().contains("cmdSave_Click"));
    }
}
