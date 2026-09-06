//! Standalone and embedded macros: a tree of submacros, `...` condition groups, typed actions.

mod accmd;

use crate::expr::{Expr, parse_expr};
use crate::saveastext::{self, Node};
use crate::{Error, Result};

pub use accmd::AcCmd;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Macro {
    pub name: String,
    pub version: Option<String>,
    pub submacros: Vec<Submacro>,
    pub text: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Submacro {
    pub name: Option<String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[allow(clippy::large_enum_variant)]
pub enum Step {
    Always(Action),
    When { cond: Expr, body: Vec<Action> },
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[allow(clippy::large_enum_variant)]
pub enum Action {
    OpenForm {
        form: String,
        view: FormView,
        filter_name: Option<String>,
        where_condition: Option<Expr>,
        data_mode: FormDataMode,
        window_mode: WindowMode,
    },
    OpenReport {
        report: String,
        view: ReportView,
        filter_name: Option<String>,
        where_condition: Option<Expr>,
        window_mode: WindowMode,
    },
    OpenQuery {
        query: String,
        view: View,
        data_mode: DataMode,
    },
    OpenTable {
        table: String,
        view: View,
        data_mode: DataMode,
    },
    RunCommand(AcCmd),
    RunMacro {
        name: String,
    },
    SetTempVar {
        name: String,
        value: Expr,
    },
    RemoveTempVar {
        name: String,
    },
    ApplyFilter {
        filter_name: Option<String>,
        where_condition: Option<Expr>,
    },
    SetValue {
        item: Expr,
        expr: Expr,
    },
    SetProperty {
        control: String,
        property: PropertyNum,
        value: Expr,
    },
    SetWarnings {
        on: bool,
    },
    MsgBox {
        message: Expr,
        beep: bool,
        type_: MsgBoxType,
        title: Option<String>,
    },
    SendObject {
        object_type: ObjectType,
        object_name: Option<String>,
        format: Option<String>,
        to: Option<Expr>,
        cc: Option<Expr>,
        bcc: Option<Expr>,
        subject: Option<Expr>,
        message: Option<Expr>,
        edit_message: bool,
    },
    Close {
        object_type: ObjectType,
        object_name: Option<String>,
        save: SaveMode,
    },
    Requery {
        control: Option<String>,
    },
    GoToRecord {
        object_type: ObjectType,
        object_name: Option<String>,
        record: Record,
        offset: Option<i32>,
    },
    GoToControl {
        control: String,
    },
    SearchForRecord {
        object_type: ObjectType,
        object_name: Option<String>,
        record: Record,
        where_condition: Option<Expr>,
    },
    MoveSize {
        right: Option<i32>,
        down: Option<i32>,
        width: Option<i32>,
        height: Option<i32>,
    },
    Beep,
    StopMacro,
    CancelEvent,
    OnError {
        next: ErrorNext,
        macro_name: Option<String>,
    },
    ClearMacroError,
    Comment(String),
    Unknown {
        name: String,
        arguments: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum FormView {
    Normal,
    Design,
    PrintPreview,
    Datasheet,
    PivotTable,
    PivotChart,
    Layout,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ReportView {
    Normal,
    Design,
    Preview,
    PivotTable,
    PivotChart,
    Report,
    Layout,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum View {
    Normal,
    Design,
    Preview,
    Datasheet,
    PivotTable,
    PivotChart,
    Layout,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum FormDataMode {
    Add,
    Edit,
    ReadOnly,
    PropertySettings,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DataMode {
    Add,
    Edit,
    ReadOnly,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum WindowMode {
    Normal,
    Hidden,
    Icon,
    Dialog,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ObjectType {
    None,
    Table,
    Query,
    Form,
    Report,
    Macro,
    Module,
    DataAccessPage,
    ServerView,
    Diagram,
    StoredProcedure,
    Function,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum SaveMode {
    No,
    Prompt,
    Yes,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum Record {
    Previous,
    Next,
    First,
    Last,
    GoTo,
    New,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum MsgBoxType {
    None,
    Critical,
    Warning,
    Information,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ErrorNext {
    Next,
    Macro,
    Fail,
    Unknown(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum PropertyNum {
    Enabled,
    Visible,
    Locked,
    Left,
    Top,
    Width,
    Height,
    ForeColor,
    BackColor,
    Caption,
    Unknown(i32),
}

impl Macro {
    pub fn parse(name: &str, text: String) -> Result<Macro> {
        let doc = saveastext::parse(&text);
        Ok(Macro {
            name: name.to_string(),
            version: doc.get("Version").map(String::from),
            submacros: submacros_of(&doc.blocks)?,
            text,
        })
    }

    pub fn from_node(name: &str, node: &Node) -> Result<Macro> {
        Ok(Macro {
            name: name.to_string(),
            version: node.get("Version").map(String::from),
            submacros: submacros_of(&node.children)?,
            text: String::new(),
        })
    }

    pub fn submacro(&self, name: &str) -> Option<&Submacro> {
        self.submacros.iter().find(|s| {
            s.name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
    }

    /// First named submacro, or the unnamed prefix if the macro has no names.
    pub fn entry(&self) -> &Submacro {
        self.submacros
            .iter()
            .find(|s| s.name.is_some())
            .or_else(|| self.submacros.first())
            .expect("Macro always has at least one submacro")
    }
}

fn submacros_of(blocks: &[Node]) -> Result<Vec<Submacro>> {
    struct Cur {
        name: Option<String>,
        nodes: Vec<Node>,
    }
    let mut cur: Option<Cur> = None;
    let mut out = Vec::new();
    let flush = |cur: &mut Option<Cur>, out: &mut Vec<Submacro>| -> Result<()> {
        if let Some(c) = cur.take() {
            out.push(Submacro {
                name: c.name,
                steps: steps_from_nodes(&c.nodes)?,
            });
        }
        Ok(())
    };
    for b in blocks {
        let mname = b.get("MacroName").filter(|s| !s.is_empty());
        let payload = is_payload(b);
        if mname.is_none() && !payload {
            continue;
        }
        if let Some(name) = mname {
            flush(&mut cur, &mut out)?;
            let mut nodes = Vec::new();
            if payload {
                nodes.push(b.clone());
            }
            cur = Some(Cur {
                name: Some(name.to_string()),
                nodes,
            });
        } else {
            cur.get_or_insert_with(|| Cur {
                name: None,
                nodes: Vec::new(),
            })
            .nodes
            .push(b.clone());
        }
    }
    flush(&mut cur, &mut out)?;
    if out.is_empty() {
        out.push(Submacro {
            name: None,
            steps: Vec::new(),
        });
    }
    Ok(out)
}

fn is_payload(n: &Node) -> bool {
    n.get("Action").is_some()
        || n.get("Comment").is_some()
        || matches!(n.kind.as_str(), "If" | "ElseIf" | "Else")
        || !n.children.is_empty() && matches!(n.kind.as_str(), "If" | "ElseIf" | "Else")
}

fn steps_from_nodes(nodes: &[Node]) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    let mut i = 0;
    while i < nodes.len() {
        let n = &nodes[i];
        if n.kind == "If" || n.kind == "ElseIf" {
            let cond = slot_expr(n.get("Condition").unwrap_or(""))?;
            let body = action_list(&n.children)?;
            steps.push(Step::When { cond, body });
            i += 1;
            continue;
        }
        if n.kind == "Else" {
            for a in action_list(&n.children)? {
                steps.push(Step::Always(a));
            }
            i += 1;
            continue;
        }
        if !is_payload(n) {
            i += 1;
            continue;
        }
        match n.get("Condition") {
            Some("...") => {
                return Err(Error::Invalid {
                    part: "macro".into(),
                    reason: "condition '...' with no preceding predicate".into(),
                });
            }
            Some(cond) => {
                let expr = slot_expr(cond)?;
                let mut body = decode_node(n)?;
                i += 1;
                while i < nodes.len() && nodes[i].get("Condition") == Some("...") {
                    body.extend(decode_node(&nodes[i])?);
                    i += 1;
                }
                steps.push(Step::When { cond: expr, body });
            }
            None => {
                for a in decode_node(n)? {
                    steps.push(Step::Always(a));
                }
                i += 1;
            }
        }
    }
    Ok(steps)
}

fn action_list(nodes: &[Node]) -> Result<Vec<Action>> {
    let mut out = Vec::new();
    for n in nodes {
        if is_payload(n) {
            out.extend(decode_node(n)?);
        }
    }
    Ok(out)
}

fn decode_node(n: &Node) -> Result<Vec<Action>> {
    if n.get("Action").is_none() {
        if let Some(c) = n.get("Comment").filter(|c| !c.is_empty()) {
            return Ok(vec![Action::Comment(c.to_string())]);
        }
        return Ok(Vec::new());
    }
    let name = n.get("Action").unwrap();
    Ok(vec![decode_action(name, n)?])
}

fn decode_action(name: &str, n: &Node) -> Result<Action> {
    let a = |schema: &[&str]| arg_slots(n, schema);
    match name {
        "OpenForm" => {
            let p = a(&[
                "FormName",
                "View",
                "FilterName",
                "WhereCondition",
                "DataMode",
                "WindowMode",
            ]);
            Ok(Action::OpenForm {
                form: p[0].clone(),
                view: FormView::from_code(&p[1]),
                filter_name: nonempty(&p[2]),
                where_condition: opt_expr(&p[3])?,
                data_mode: FormDataMode::from_code(&p[4]),
                window_mode: WindowMode::from_code(&p[5]),
            })
        }
        "OpenReport" => {
            let p = a(&[
                "ReportName",
                "View",
                "FilterName",
                "WhereCondition",
                "WindowMode",
            ]);
            Ok(Action::OpenReport {
                report: p[0].clone(),
                view: ReportView::from_code(&p[1]),
                filter_name: nonempty(&p[2]),
                where_condition: opt_expr(&p[3])?,
                window_mode: WindowMode::from_code(&p[4]),
            })
        }
        "OpenQuery" => {
            let p = a(&["QueryName", "View", "DataMode"]);
            Ok(Action::OpenQuery {
                query: p[0].clone(),
                view: View::from_code(&p[1]),
                data_mode: DataMode::from_code(&p[2]),
            })
        }
        "OpenTable" => {
            let p = a(&["TableName", "View", "DataMode"]);
            Ok(Action::OpenTable {
                table: p[0].clone(),
                view: View::from_code(&p[1]),
                data_mode: DataMode::from_code(&p[2]),
            })
        }
        "RunCommand" => {
            let p = a(&["Command"]);
            let n = p[0].parse::<u16>().unwrap_or(0);
            Ok(Action::RunCommand(AcCmd::from(n)))
        }
        "RunMacro" => {
            let p = a(&["MacroName"]);
            Ok(Action::RunMacro { name: p[0].clone() })
        }
        "SetTempVar" => {
            let p = a(&["Name", "Expression"]);
            Ok(Action::SetTempVar {
                name: p[0].clone(),
                value: slot_expr(&p[1])?,
            })
        }
        "RemoveTempVar" => {
            let p = a(&["Name"]);
            Ok(Action::RemoveTempVar { name: p[0].clone() })
        }
        "ApplyFilter" | "SetFilter" => {
            let p = a(&["FilterName", "WhereCondition"]);
            Ok(Action::ApplyFilter {
                filter_name: nonempty(&p[0]),
                where_condition: opt_expr(&p[1])?,
            })
        }
        "SetValue" => {
            let p = a(&["Item", "Expression"]);
            Ok(Action::SetValue {
                item: slot_expr(&p[0])?,
                expr: slot_expr(&p[1])?,
            })
        }
        "SetProperty" => {
            let p = a(&["ControlName", "Property", "Value"]);
            Ok(Action::SetProperty {
                control: p[0].clone(),
                property: PropertyNum::from_code(&p[1]),
                value: slot_expr(&p[2])?,
            })
        }
        "SetWarnings" => {
            let p = a(&["WarningsOn"]);
            Ok(Action::SetWarnings {
                on: access_bool(&p[0]),
            })
        }
        "MsgBox" => {
            let p = a(&["Message", "Beep", "Type", "Title"]);
            Ok(Action::MsgBox {
                message: slot_expr(&p[0])?,
                beep: access_bool(&p[1]),
                type_: MsgBoxType::from_code(&p[2]),
                title: nonempty(&p[3]),
            })
        }
        "SendObject" => {
            let p = a(&[
                "ObjectType",
                "ObjectName",
                "OutputFormat",
                "To",
                "Cc",
                "Bcc",
                "Subject",
                "MessageText",
                "EditMessage",
            ]);
            Ok(Action::SendObject {
                object_type: ObjectType::from_code(&p[0]),
                object_name: nonempty(&p[1]),
                format: nonempty(&p[2]),
                to: opt_expr(&p[3])?,
                cc: opt_expr(&p[4])?,
                bcc: opt_expr(&p[5])?,
                subject: opt_expr(&p[6])?,
                message: opt_expr(&p[7])?,
                edit_message: access_bool(&p[8]),
            })
        }
        "Close" => {
            let p = a(&["ObjectType", "ObjectName", "Save"]);
            Ok(Action::Close {
                object_type: ObjectType::from_code(&p[0]),
                object_name: nonempty(&p[1]),
                save: SaveMode::from_code(&p[2]),
            })
        }
        "Requery" => {
            let p = a(&["ControlName"]);
            Ok(Action::Requery {
                control: nonempty(&p[0]),
            })
        }
        "GoToRecord" => {
            let p = a(&["ObjectType", "ObjectName", "Record", "Offset"]);
            Ok(Action::GoToRecord {
                object_type: ObjectType::from_code(&p[0]),
                object_name: nonempty(&p[1]),
                record: Record::from_code(&p[2]),
                offset: parse_i32(&p[3]),
            })
        }
        "GoToControl" => {
            let p = a(&["ControlName"]);
            Ok(Action::GoToControl {
                control: p[0].clone(),
            })
        }
        "SearchForRecord" => {
            let p = a(&["ObjectType", "ObjectName", "Record", "WhereCondition"]);
            Ok(Action::SearchForRecord {
                object_type: ObjectType::from_code(&p[0]),
                object_name: nonempty(&p[1]),
                record: Record::from_code(&p[2]),
                where_condition: opt_expr(&p[3])?,
            })
        }
        "MoveSize" => {
            let p = a(&["Right", "Down", "Width", "Height"]);
            Ok(Action::MoveSize {
                right: parse_i32(&p[0]),
                down: parse_i32(&p[1]),
                width: parse_i32(&p[2]),
                height: parse_i32(&p[3]),
            })
        }
        "Beep" => Ok(Action::Beep),
        "StopMacro" => Ok(Action::StopMacro),
        "CancelEvent" => Ok(Action::CancelEvent),
        "OnError" => {
            let p = a(&["GoTo", "MacroName"]);
            Ok(Action::OnError {
                next: ErrorNext::from_code(&p[0]),
                macro_name: nonempty(&p[1]),
            })
        }
        "ClearMacroError" => Ok(Action::ClearMacroError),
        other => Ok(Action::Unknown {
            name: other.to_string(),
            arguments: n.values("Argument").into_iter().map(String::from).collect(),
        }),
    }
}

fn arg_slots(n: &Node, schema: &[&str]) -> Vec<String> {
    let vals: Vec<&str> = n.values("Argument");
    let names: Vec<&str> = n.values("ArgumentName");
    let named = names.iter().any(|s| !s.is_empty());
    if named {
        let mut slots = vec![String::new(); schema.len()];
        for (name, val) in names.iter().zip(vals.iter()) {
            if let Some(i) = schema.iter().position(|s| s.eq_ignore_ascii_case(name)) {
                slots[i] = (*val).to_string();
            }
        }
        slots
    } else {
        let mut slots = vec![String::new(); schema.len()];
        for (i, v) in vals.iter().take(schema.len()).enumerate() {
            slots[i] = (*v).to_string();
        }
        slots
    }
}

fn nonempty(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn parse_i32(s: &str) -> Option<i32> {
    s.trim().parse().ok()
}

fn access_bool(s: &str) -> bool {
    matches!(s.trim(), "-1" | "True" | "true" | "1" | "Yes" | "yes")
}

fn slot_expr(s: &str) -> Result<Expr> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Expr::Null);
    }
    parse_expr(s).or_else(|_| Ok(Expr::Text(s.to_string())))
}

fn opt_expr(s: &str) -> Result<Option<Expr>> {
    if s.trim().is_empty() {
        Ok(None)
    } else {
        slot_expr(s).map(Some)
    }
}

fn code_i32(s: &str) -> Option<i32> {
    s.trim().parse().ok()
}

impl FormView {
    fn from_code(s: &str) -> FormView {
        match code_i32(s).unwrap_or(0) {
            0 => FormView::Normal,
            1 => FormView::Design,
            2 => FormView::PrintPreview,
            3 => FormView::Datasheet,
            4 => FormView::PivotTable,
            5 => FormView::PivotChart,
            6 => FormView::Layout,
            n => FormView::Unknown(n),
        }
    }
}

impl ReportView {
    fn from_code(s: &str) -> ReportView {
        match code_i32(s).unwrap_or(0) {
            0 => ReportView::Normal,
            1 => ReportView::Design,
            2 => ReportView::Preview,
            3 => ReportView::PivotTable,
            4 => ReportView::PivotChart,
            5 => ReportView::Report,
            6 => ReportView::Layout,
            n => ReportView::Unknown(n),
        }
    }
}

impl View {
    fn from_code(s: &str) -> View {
        match code_i32(s).unwrap_or(0) {
            0 => View::Normal,
            1 => View::Design,
            2 => View::Preview,
            3 => View::Datasheet,
            4 => View::PivotTable,
            5 => View::PivotChart,
            6 => View::Layout,
            n => View::Unknown(n),
        }
    }
}

impl FormDataMode {
    fn from_code(s: &str) -> FormDataMode {
        match code_i32(s) {
            None | Some(-1) => FormDataMode::PropertySettings,
            Some(0) => FormDataMode::Add,
            Some(1) => FormDataMode::Edit,
            Some(2) => FormDataMode::ReadOnly,
            Some(n) => FormDataMode::Unknown(n),
        }
    }
}

impl DataMode {
    fn from_code(s: &str) -> DataMode {
        match code_i32(s).unwrap_or(1) {
            0 => DataMode::Add,
            1 => DataMode::Edit,
            2 => DataMode::ReadOnly,
            n => DataMode::Unknown(n),
        }
    }
}

impl WindowMode {
    fn from_code(s: &str) -> WindowMode {
        match code_i32(s).unwrap_or(0) {
            0 => WindowMode::Normal,
            1 => WindowMode::Hidden,
            2 => WindowMode::Icon,
            3 => WindowMode::Dialog,
            n => WindowMode::Unknown(n),
        }
    }
}

impl ObjectType {
    fn from_code(s: &str) -> ObjectType {
        match code_i32(s).unwrap_or(-1) {
            -1 => ObjectType::None,
            0 => ObjectType::Table,
            1 => ObjectType::Query,
            2 => ObjectType::Form,
            3 => ObjectType::Report,
            4 => ObjectType::Macro,
            5 => ObjectType::Module,
            6 => ObjectType::DataAccessPage,
            7 => ObjectType::ServerView,
            8 => ObjectType::Diagram,
            9 => ObjectType::StoredProcedure,
            10 => ObjectType::Function,
            n => ObjectType::Unknown(n),
        }
    }
}

impl SaveMode {
    fn from_code(s: &str) -> SaveMode {
        match code_i32(s).unwrap_or(1) {
            0 => SaveMode::No,
            1 => SaveMode::Prompt,
            2 => SaveMode::Yes,
            n => SaveMode::Unknown(n),
        }
    }
}

impl Record {
    fn from_code(s: &str) -> Record {
        match code_i32(s).unwrap_or(1) {
            0 => Record::Previous,
            1 => Record::Next,
            2 => Record::First,
            3 => Record::Last,
            4 => Record::GoTo,
            5 => Record::New,
            n => Record::Unknown(n),
        }
    }
}

impl MsgBoxType {
    fn from_code(s: &str) -> MsgBoxType {
        match code_i32(s).unwrap_or(0) {
            0 => MsgBoxType::None,
            1 => MsgBoxType::Critical,
            2 => MsgBoxType::Warning,
            4 => MsgBoxType::Information,
            n => MsgBoxType::Unknown(n),
        }
    }
}

impl ErrorNext {
    fn from_code(s: &str) -> ErrorNext {
        match code_i32(s).unwrap_or(0) {
            0 => ErrorNext::Next,
            1 => ErrorNext::Macro,
            2 => ErrorNext::Fail,
            n => ErrorNext::Unknown(n),
        }
    }
}

impl PropertyNum {
    fn from_code(s: &str) -> PropertyNum {
        match code_i32(s).unwrap_or(-1) {
            0 => PropertyNum::Enabled,
            1 => PropertyNum::Visible,
            2 => PropertyNum::Locked,
            3 => PropertyNum::Left,
            4 => PropertyNum::Top,
            5 => PropertyNum::Width,
            6 => PropertyNum::Height,
            7 => PropertyNum::ForeColor,
            8 => PropertyNum::BackColor,
            9 => PropertyNum::Caption,
            n => PropertyNum::Unknown(n),
        }
    }
}
