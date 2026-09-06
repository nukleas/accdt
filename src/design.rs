//! Forms and reports: the SaveAsText design plus derived views (controls, events,
//! embedded macros, code-behind).

use std::collections::BTreeMap;

use crate::macros::Macro;
use crate::saveastext::{self, Document, Node};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DesignKind {
    Form,
    Report,
}

const SECTION_KINDS: &[&str] = &[
    "FormHeader",
    "FormFooter",
    "PageHeader",
    "PageFooter",
    "ReportHeader",
    "ReportFooter",
    "GroupHeader",
    "GroupFooter",
    "Section",
    "Detail",
    "BreakHeader",
    "BreakFooter",
];

/// Event properties that do not start with `On`.
const BARE_EVENTS: &[&str] = &[
    "AfterUpdate",
    "BeforeUpdate",
    "AfterInsert",
    "BeforeInsert",
    "AfterDelConfirm",
    "BeforeDelConfirm",
    "AfterLayout",
    "AfterRender",
    "AfterFinalRender",
    "BeforeRender",
    "BeforeQuery",
    "BeforeScreenTip",
    "BeforeNavigate",
    "AfterNavigate",
];

/// Stable ordinal in a design's mixed item walk. Compare only within that design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct NodeId(usize);
impl NodeId {
    pub fn index(self) -> usize {
        self.0
    }
}

/// A borrowed control bound to its authoritative design node.
#[derive(Debug, Clone, Copy)]
pub struct Control<'d> {
    design: &'d Design,
    node: &'d Node,
}
#[derive(Debug, Clone, Copy)]
pub struct Section<'d> {
    design: &'d Design,
    node: &'d Node,
}
#[derive(Debug, Clone, Copy)]
pub enum DesignItem<'d> {
    Control(Control<'d>),
    Section(Section<'d>),
}

impl<'d> DesignItem<'d> {
    pub fn id(self) -> NodeId {
        match self {
            Self::Control(c) => c.id(),
            Self::Section(s) => s.id(),
        }
    }
    pub fn name(self) -> &'d str {
        self.raw_node().get("Name").unwrap_or("")
    }
    pub fn raw_node(self) -> &'d Node {
        match self {
            Self::Control(c) => c.node,
            Self::Section(s) => s.node,
        }
    }
}
impl<'d> Section<'d> {
    pub fn id(self) -> NodeId {
        NodeId(
            self.design
                .items()
                .iter()
                .position(|i| std::ptr::eq(i.raw_node(), self.node))
                .expect("design-bound node"),
        )
    }
    pub fn name(self) -> &'d str {
        self.node.get("Name").unwrap_or("")
    }
    pub fn raw_node(self) -> &'d Node {
        self.node
    }
    pub fn kind(self) -> SectionKind {
        match self.node.kind.as_str() {
            "Section" | "Detail" => SectionKind::Detail,
            "FormHeader" if self.design.kind == DesignKind::Form => SectionKind::FormHeader,
            "FormFooter" if self.design.kind == DesignKind::Form => SectionKind::FormFooter,
            "FormHeader" | "ReportHeader" => SectionKind::ReportHeader,
            "FormFooter" | "ReportFooter" => SectionKind::ReportFooter,
            "PageHeader" => SectionKind::PageHeader,
            "PageFooter" => SectionKind::PageFooter,
            "BreakHeader" | "GroupHeader" => SectionKind::GroupHeader,
            "BreakFooter" | "GroupFooter" => SectionKind::GroupFooter,
            other => SectionKind::Unknown(other.into()),
        }
    }
}
impl<'d> Control<'d> {
    pub fn id(self) -> NodeId {
        NodeId(
            self.design
                .items()
                .iter()
                .position(|i| std::ptr::eq(i.raw_node(), self.node))
                .expect("design-bound node"),
        )
    }
    pub fn name(self) -> &'d str {
        self.node.get("Name").unwrap_or("")
    }
    pub fn raw_node(self) -> &'d Node {
        self.node
    }
    pub fn raw_property(self, name: &str) -> Option<&'d str> {
        self.node.get(name)
    }
    pub fn kind(self) -> ControlKind {
        ControlKind::from_name(&self.node.kind)
    }
    pub fn parent(self) -> Option<DesignItem<'d>> {
        self.design.ancestry(self.node).last().copied()
    }
    pub fn section(self) -> Option<Section<'d>> {
        self.design
            .ancestry(self.node)
            .into_iter()
            .rev()
            .find_map(|i| {
                if let DesignItem::Section(s) = i {
                    Some(s)
                } else {
                    None
                }
            })
    }
    /// Structural attachment only: labels on tab Pages can still be explanatory text.
    pub fn is_attached_label(self) -> bool {
        self.kind() == ControlKind::Label && matches!(self.parent(), Some(DesignItem::Control(_)))
    }
    /// Name convention, not proof that a label is disposable.
    pub fn is_layout_label_candidate(self) -> bool {
        self.kind() == ControlKind::Label && self.name().contains("_LayoutLabel")
    }
    pub fn embedded_macros(self) -> BTreeMap<String, Macro> {
        embedded_macros_of(self.node)
    }
    fn effective(self, key: &str) -> Option<Resolved<&'d str>> {
        self.raw_property(key)
            .map(|value| Resolved {
                value,
                origin: PropertyOrigin::Explicit,
            })
            .or_else(|| {
                self.design
                    .control_defaults()
                    .get(self.node.kind.as_str())
                    .and_then(|p| p.get(key))
                    .map(|value| Resolved {
                        value: value.as_str(),
                        origin: PropertyOrigin::ControlDefault,
                    })
            })
    }
    fn number<T>(self, key: &str, decode: impl FnOnce(i32) -> T) -> PropertyResult<T> {
        self.effective(key)
            .map(|r| integer(self.design.name(), self.name(), key, r, decode))
            .transpose()
    }
    fn boolean(self, key: &str, baseline: bool) -> PropertyResult<bool> {
        let known = !matches!(
            self.kind(),
            ControlKind::UnknownName(_) | ControlKind::UnknownCode(_)
        );
        let applicable = match key {
            "Visible" => known,
            "Enabled" => matches!(
                self.kind(),
                ControlKind::TextBox
                    | ControlKind::ComboBox
                    | ControlKind::ListBox
                    | ControlKind::CheckBox
                    | ControlKind::CommandButton
                    | ControlKind::Subform
                    | ControlKind::OptionGroup
                    | ControlKind::OptionButton
                    | ControlKind::ToggleButton
                    | ControlKind::Attachment
                    | ControlKind::Tab
                    | ControlKind::Page
            ),
            "Locked" => matches!(
                self.kind(),
                ControlKind::TextBox
                    | ControlKind::ComboBox
                    | ControlKind::ListBox
                    | ControlKind::CheckBox
                    | ControlKind::Subform
                    | ControlKind::OptionGroup
                    | ControlKind::OptionButton
                    | ControlKind::ToggleButton
                    | ControlKind::Attachment
            ),
            "ColumnHidden" => matches!(
                self.kind(),
                ControlKind::TextBox
                    | ControlKind::ComboBox
                    | ControlKind::CheckBox
                    | ControlKind::Attachment
            ),
            _ => false,
        };
        let baseline = (!self.design.is_axl() && applicable).then_some(baseline);
        let raw = self.effective(key);
        if raw
            .as_ref()
            .is_some_and(|r| r.value == "NotDefault" && r.origin == PropertyOrigin::Explicit)
            && self
                .design
                .control_defaults()
                .get(self.node.kind.as_str())
                .is_some_and(|p| p.contains_key(key))
        {
            return Err(PropertyError::new(
                self.design.name(),
                self.name(),
                key,
                "NotDefault",
                "custom default inversion requires paired export evidence",
            ));
        }
        boolean(self.design.name(), self.name(), key, raw, baseline)
    }
    pub fn is_visible(self) -> PropertyResult<bool> {
        self.boolean("Visible", true)
    }
    pub fn is_enabled(self) -> PropertyResult<bool> {
        self.boolean("Enabled", true)
    }
    pub fn is_locked(self) -> PropertyResult<bool> {
        self.boolean("Locked", false)
    }
    pub fn column_hidden(self) -> PropertyResult<bool> {
        self.boolean("ColumnHidden", false)
    }
    pub fn text_align(self) -> PropertyResult<TextAlign> {
        self.number("TextAlign", TextAlign::from_code)
    }
    pub fn back_style(self) -> PropertyResult<BackStyle> {
        self.number("BackStyle", BackStyle::from_code)
    }
    pub fn decimal_places(self) -> PropertyResult<DecimalPlaces> {
        self.number("DecimalPlaces", DecimalPlaces::from_code)
    }
    pub fn format(self) -> Option<DisplayFormat<'d>> {
        self.effective("Format")
            .map(|r| DisplayFormat::parse(r.value))
    }
    pub fn input_mask(self) -> Option<&'d str> {
        self.effective("InputMask").map(|r| r.value)
    }
    pub fn layout(self) -> Result<Layout, PropertyError> {
        let n = |key| {
            self.effective(key)
                .map(|r| {
                    r.value.parse::<i64>().map_err(|_| {
                        PropertyError::new(
                            self.design.name(),
                            self.name(),
                            key,
                            r.value,
                            "expected integer layout value",
                        )
                    })
                })
                .transpose()
        };
        Ok(Layout {
            left: n("Left")?,
            top: n("Top")?,
            width: n("Width")?,
            height: n("Height")?,
        })
    }
}

pub type PropertyResult<T> = Result<Option<Resolved<T>>, PropertyError>;
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Resolved<T> {
    pub value: T,
    pub origin: PropertyOrigin,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum PropertyOrigin {
    Explicit,
    ControlDefault,
    FormatDefault,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{design}/{node}: {property}={raw:?}: {reason}")]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PropertyError {
    pub design: String,
    pub node: String,
    pub property: String,
    pub raw: String,
    pub reason: String,
}
impl PropertyError {
    pub(crate) fn new(design: &str, node: &str, property: &str, raw: &str, reason: &str) -> Self {
        Self {
            design: design.into(),
            node: node.into(),
            property: property.into(),
            raw: raw.into(),
            reason: reason.into(),
        }
    }
}
pub(crate) fn integer<T>(
    design: &str,
    node: &str,
    key: &str,
    r: Resolved<&str>,
    decode: impl FnOnce(i32) -> T,
) -> Result<Resolved<T>, PropertyError> {
    let value = r
        .value
        .parse()
        .map_err(|_| PropertyError::new(design, node, key, r.value, "expected integer"))?;
    Ok(Resolved {
        value: decode(value),
        origin: r.origin,
    })
}
fn boolean(
    design: &str,
    node: &str,
    key: &str,
    raw: Option<Resolved<&str>>,
    baseline: Option<bool>,
) -> PropertyResult<bool> {
    let Some(raw) = raw else {
        return Ok(baseline.map(|value| Resolved {
            value,
            origin: PropertyOrigin::FormatDefault,
        }));
    };
    let value = match raw.value {
        "0" | "false" | "False" => false,
        "-1" | "1" | "true" | "True" => true,
        "NotDefault" => !baseline.ok_or_else(|| {
            PropertyError::new(
                design,
                node,
                key,
                raw.value,
                "unverified serialization baseline",
            )
        })?,
        _ => {
            return Err(PropertyError::new(
                design,
                node,
                key,
                raw.value,
                "expected boolean",
            ));
        }
    };
    Ok(Some(Resolved {
        value,
        origin: raw.origin,
    }))
}
macro_rules! coded_enum {
    ($name:ident { $($variant:ident = $code:literal),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize))]
        pub enum $name { $($variant,)* Unknown(i32) }
        impl $name { pub fn from_code(code: i32) -> Self { match code { $($code => Self::$variant,)* n => Self::Unknown(n) } } }
    };
}
coded_enum!(DefaultView { Single=0, Continuous=1, Datasheet=2, PivotTable=3, PivotChart=4, Split=5 });
coded_enum!(TextAlign { General=0, Left=1, Center=2, Right=3, Distribute=4 });
coded_enum!(BackStyle { Transparent=0, Normal=1 });
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DecimalPlaces {
    Auto,
    Fixed(u8),
    Unknown(i32),
}
impl DecimalPlaces {
    pub fn from_code(n: i32) -> Self {
        match n {
            255 => Self::Auto,
            0..=15 => Self::Fixed(n as u8),
            _ => Self::Unknown(n),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum SectionKind {
    Detail,
    FormHeader,
    FormFooter,
    ReportHeader,
    ReportFooter,
    PageHeader,
    PageFooter,
    GroupHeader,
    GroupFooter,
    Unknown(String),
}
macro_rules! control_kinds {
    ($($variant:ident = $code:literal),* $(,)?) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize))]
        pub enum ControlKind { $($variant,)* UnknownName(String), UnknownCode(i32) }
        impl ControlKind {
            pub fn from_code(n: i32) -> Self { match n { $($code => Self::$variant,)* n => Self::UnknownCode(n) } }
            pub fn from_name(s: &str) -> Self { $(if s.eq_ignore_ascii_case(stringify!($variant)) { return Self::$variant; })* Self::UnknownName(s.into()) }
        }
    };
}
control_kinds!(
    Label = 100,
    Rectangle = 101,
    Line = 102,
    Image = 103,
    CommandButton = 104,
    OptionButton = 105,
    CheckBox = 106,
    OptionGroup = 107,
    BoundObjectFrame = 108,
    TextBox = 109,
    ListBox = 110,
    ComboBox = 111,
    Subform = 112,
    ObjectFrame = 114,
    PageBreak = 118,
    CustomControl = 119,
    ToggleButton = 122,
    Tab = 123,
    Page = 124,
    Attachment = 126,
    EmptyCell = 127,
    WebBrowser = 128,
    NavigationControl = 129,
    NavigationButton = 130,
    Chart = 133,
    EdgeBrowser = 134
);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum NamedFormat {
    GeneralDate,
    LongDate,
    MediumDate,
    ShortDate,
    LongTime,
    MediumTime,
    ShortTime,
    GeneralNumber,
    Currency,
    Euro,
    Fixed,
    Standard,
    Percent,
    Scientific,
    YesNo,
    TrueFalse,
    OnOff,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum DisplayFormat<'a> {
    Named(NamedFormat),
    Custom(&'a str),
}
impl<'a> DisplayFormat<'a> {
    pub fn parse(s: &'a str) -> Self {
        use NamedFormat::*;
        let named = match s.to_ascii_lowercase().as_str() {
            "general date" => GeneralDate,
            "long date" => LongDate,
            "medium date" => MediumDate,
            "short date" => ShortDate,
            "long time" => LongTime,
            "medium time" => MediumTime,
            "short time" => ShortTime,
            "general number" => GeneralNumber,
            "currency" => Currency,
            "euro" => Euro,
            "fixed" => Fixed,
            "standard" => Standard,
            "percent" => Percent,
            "scientific" => Scientific,
            "yes/no" => YesNo,
            "true/false" => TrueFalse,
            "on/off" => OnOff,
            _ => return Self::Custom(s),
        };
        Self::Named(named)
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
    /// `Click`, `Load`, `AfterUpdate`, … (the property name without a leading `On`).
    pub event: String,
    /// `[Event Procedure]`, `[Embedded Macro]`, a macro name, or an `=expression`.
    pub value: String,
}

impl Event {
    /// The VBA procedure name Access binds for `[Event Procedure]`.
    pub fn procedure_name(&self) -> Option<String> {
        (self.value == "[Event Procedure]")
            .then(|| format!("{}_{}", self.owner.replace(' ', "_"), self.event))
    }
}

/// A form or report: the parsed SaveAsText document plus typed views over it.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Design {
    name: String,
    kind: DesignKind,
    /// The parsed document; every other accessor is a view over it.
    document: Document,
    /// The original SaveAsText text.
    text: String,
}

static EMPTY: std::sync::LazyLock<BTreeMap<String, String>> =
    std::sync::LazyLock::new(BTreeMap::new);

impl Design {
    /// Parse a form or report from SaveAsText text (a template part, or `Application.SaveAsText` output).
    pub fn parse(name: &str, kind: DesignKind, text: String) -> Design {
        Design {
            name: name.to_string(),
            kind,
            document: saveastext::parse(&text),
            text,
        }
    }

    /// Parse an Access Services (web database) form or report from its AXL XML.
    pub fn parse_axl(name: &str, kind: DesignKind, xml: String) -> crate::Result<Design> {
        let document = match kind {
            DesignKind::Form => crate::axl::form_document(name, &xml)?,
            DesignKind::Report => crate::axl::report_document(name, &xml)?,
        };
        Ok(Design {
            name: name.to_string(),
            kind,
            document,
            text: xml,
        })
    }

    /// True when the design came from AXL (web database) rather than SaveAsText.
    pub fn is_axl(&self) -> bool {
        self.document.get("Format") == Some("axl")
    }

    fn root(&self) -> Option<&Node> {
        self.document.blocks.first()
    }

    /// Header lines (`Version`, `Checksum`, …).
    pub fn header(&self) -> &BTreeMap<String, String> {
        &self.document.header
    }

    /// Form/report level properties (`RecordSource`, `Caption`, `DefaultView`, …). Boolean
    /// properties whose value differs from Access's default are written as the literal
    /// `NotDefault`; which boolean that means depends on the property's default.
    pub fn properties(&self) -> &BTreeMap<String, String> {
        self.root().map(|r| &r.properties).unwrap_or(&EMPTY)
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.properties().get(key).map(String::as_str)
    }

    pub fn record_source(&self) -> Option<&str> {
        self.get("RecordSource")
    }

    /// VBA source of the code-behind module, if the object has one.
    pub fn code_behind(&self) -> Option<&str> {
        self.document
            .code_behind
            .as_deref()
            .filter(|c| !c.trim().is_empty())
    }

    /// Per-control-type default properties from the design's anonymous defaults block.
    pub fn control_defaults(&self) -> BTreeMap<&str, &BTreeMap<String, String>> {
        let mut out = BTreeMap::new();
        for block in self
            .root()
            .into_iter()
            .flat_map(|r| r.children.iter())
            .filter(|c| c.kind == "Block")
        {
            for d in &block.children {
                if d.get("Name").is_none() && !SECTION_KINDS.contains(&d.kind.as_str()) {
                    out.insert(d.kind.as_str(), &d.properties);
                }
            }
        }
        out
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn kind(&self) -> DesignKind {
        self.kind
    }
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn default_view(&self) -> PropertyResult<DefaultView> {
        self.get("DefaultView")
            .map(|value| {
                integer(
                    self.name(),
                    self.owner_name(),
                    "DefaultView",
                    Resolved {
                        value,
                        origin: PropertyOrigin::Explicit,
                    },
                    DefaultView::from_code,
                )
            })
            .transpose()
    }
    /// Sections and controls in document order; defaults and macros are excluded.
    pub fn items(&self) -> Vec<DesignItem<'_>> {
        let mut out = Vec::new();
        for child in self.root().into_iter().flat_map(|r| &r.children) {
            walk(self, child, &mut out);
        }
        out
    }
    pub fn controls(&self) -> Vec<Control<'_>> {
        self.items()
            .into_iter()
            .filter_map(|i| {
                if let DesignItem::Control(c) = i {
                    Some(c)
                } else {
                    None
                }
            })
            .collect()
    }
    pub fn sections(&self) -> Vec<Section<'_>> {
        self.items()
            .into_iter()
            .filter_map(|i| {
                if let DesignItem::Section(s) = i {
                    Some(s)
                } else {
                    None
                }
            })
            .collect()
    }
    fn ancestry(&self, target: &Node) -> Vec<DesignItem<'_>> {
        fn visit<'d>(
            design: &'d Design,
            node: &'d Node,
            target: &Node,
            path: &mut Vec<DesignItem<'d>>,
        ) -> bool {
            if std::ptr::eq(node, target) {
                return true;
            }
            let item = design.item(node);
            if let Some(i) = item {
                path.push(i);
            }
            for child in &node.children {
                if visit(design, child, target, path) {
                    return true;
                }
            }
            if item.is_some() {
                path.pop();
            }
            false
        }
        let mut path = Vec::new();
        if let Some(root) = self.root() {
            for child in &root.children {
                if visit(self, child, target, &mut path) {
                    break;
                }
            }
        }
        path
    }
    fn item<'d>(&'d self, node: &'d Node) -> Option<DesignItem<'d>> {
        node.get("Name")?;
        Some(if SECTION_KINDS.contains(&node.kind.as_str()) {
            DesignItem::Section(Section { design: self, node })
        } else {
            DesignItem::Control(Control { design: self, node })
        })
    }

    fn owner_name(&self) -> &'static str {
        match self.kind {
            DesignKind::Form => "Form",
            DesignKind::Report => "Report",
        }
    }

    /// Every event property with a value (`On*`, `AfterUpdate`, `BeforeUpdate`, …), on the
    /// object and its controls.
    pub fn events(&self) -> Vec<Event> {
        let mut out = Vec::new();
        collect_events(self.owner_name(), self.properties(), &mut out);
        for c in self.items() {
            collect_events(c.name(), &c.raw_node().properties, &mut out);
        }
        out
    }

    /// Embedded macros on the object and its controls, keyed `owner.Event`.
    pub fn embedded_macros(&self) -> BTreeMap<String, Macro> {
        let mut out = BTreeMap::new();
        if let Some(root) = self.root() {
            for (event, m) in embedded_macros_of(root) {
                out.insert(format!("{}.{event}", self.owner_name()), m);
            }
        }
        for c in self.items() {
            for (event, m) in embedded_macros_of(c.raw_node()) {
                out.insert(format!("{}.{event}", c.name()), m);
            }
        }
        out
    }
}

fn embedded_macros_of(node: &Node) -> BTreeMap<String, Macro> {
    node.children
        .iter()
        .filter_map(|c| {
            c.kind
                .strip_suffix("EmMacro")
                .map(|ev| (ev.trim_start_matches("On").to_string(), c))
        })
        .map(|(event, c)| (event.clone(), Macro::from_node(&event, c)))
        .collect()
}

fn collect_events(owner: &str, props: &BTreeMap<String, String>, out: &mut Vec<Event>) {
    for (k, v) in props {
        if v.is_empty() || k.ends_with("EmMacro") {
            continue;
        }
        let event = match k.strip_prefix("On") {
            Some(ev) if ev.chars().next().is_some_and(|c| c.is_ascii_uppercase()) => ev,
            _ if BARE_EVENTS.contains(&k.as_str()) => k.as_str(),
            _ => continue,
        };
        out.push(Event {
            owner: owner.to_string(),
            event: event.to_string(),
            value: v.clone(),
        });
    }
}

fn walk<'d>(design: &'d Design, node: &'d Node, out: &mut Vec<DesignItem<'d>>) {
    if node.kind.ends_with("EmMacro") {
        return;
    }
    if node.kind != "Block" {
        let Some(item) = design.item(node) else {
            return;
        };
        out.push(item);
    }
    for child in &node.children {
        walk(design, child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORM: &str = "Version =21\nBegin Form\n    RecordSource =\"Customers\"\n    OnLoad =\"[Event Procedure]\"\n    AfterUpdate =\"[Event Procedure]\"\n    Begin\n        Begin Label\n            FontSize =11\n        End\n        Begin FormHeader\n            Name =\"FormHeader\"\n            Begin\n                Begin Label\n                    Name =\"lblTitle\"\n                    Caption =\"Customers\"\n                End\n            End\n        End\n        Begin Section\n            Name =\"Detail\"\n            Begin\n                Begin Tab\n                    Name =\"tabMain\"\n                    Begin\n                        Begin Page\n                            Name =\"pgOne\"\n                            Begin\n                                Begin CommandButton\n                                    Name =\"cmdSave\"\n                                    Left =100\n                                    Top =200\n                                    Width =1000\n                                    Height =300\n                                    OnClick =\"[Event Procedure]\"\n                                    OnDblClick =\"[Embedded Macro]\"\n                                    OnDblClickEmMacro = Begin\n                                        Version =196611\n                                        Begin\n                                            Action =\"OpenReport\"\n                                            Argument =\"rptX\"\n                                        End\n                                    End\n                                End\n                            End\n                        End\n                    End\n                End\n            End\n        End\n    End\nEnd\nCodeBehindForm\nOption Compare Database\nPrivate Sub cmdSave_Click()\nEnd Sub\n";

    #[test]
    fn controls_events_and_code() {
        let d = Design::parse("frmCustomers", DesignKind::Form, FORM.to_string());
        assert_eq!(d.record_source(), Some("Customers"));
        assert_eq!(d.control_defaults()["Label"]["FontSize"], "11");
        assert_eq!(d.header()["Version"], "21");
        let ctrls = d.controls();
        let names: Vec<&str> = ctrls.iter().map(|c| c.name()).collect();
        assert_eq!(names, vec!["lblTitle", "tabMain", "pgOne", "cmdSave"]);
        let save = ctrls.iter().find(|c| c.name() == "cmdSave").unwrap();
        assert_eq!(save.section().unwrap().name(), "Detail");
        assert_eq!(save.parent().map(DesignItem::name), Some("pgOne"));
        assert_eq!(
            save.layout().unwrap(),
            Layout {
                left: Some(100),
                top: Some(200),
                width: Some(1000),
                height: Some(300)
            }
        );
        let events = d.events();
        assert!(
            events
                .iter()
                .any(|e| e.owner == "Form" && e.event == "Load")
        );
        assert!(
            events
                .iter()
                .any(|e| e.owner == "Form" && e.event == "AfterUpdate")
        );
        let click = events
            .iter()
            .find(|e| e.owner == "cmdSave" && e.event == "Click")
            .unwrap();
        assert_eq!(click.procedure_name().as_deref(), Some("cmdSave_Click"));
        assert!(
            events
                .iter()
                .any(|e| e.event == "DblClick" && e.value == "[Embedded Macro]")
        );
        let macros = d.embedded_macros();
        assert_eq!(
            macros["cmdSave.DblClick"].actions[0].arguments,
            vec!["rptX"]
        );
        assert!(d.code_behind().unwrap().contains("cmdSave_Click"));
    }
}
