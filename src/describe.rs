//! Owned, serialisable descriptions of designs and tables: the summary a port reads first
//! (what `accdt describe` prints). Built on the typed views; raw properties stay reachable
//! through the views themselves.

use std::collections::BTreeMap;
use std::fmt;

use crate::design::{Control, ControlKind, DesignItem, GroupLevel, SectionKind};
use crate::package::{ResolvedObject, ResolvedRecordSource};
use crate::{
    BoundColumn, ColumnWidth, Design, DesignKind, DisplayFormat, Macro, Package, RowSource, Table,
};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DesignDescription {
    pub kind: DesignKind,
    pub name: String,
    /// Decoded `DefaultView`, or the raw value when it did not decode.
    pub default_view: Option<String>,
    pub record_source: SourceDescription,
    pub sections: Vec<SectionDescription>,
    pub groups: Vec<GroupDescription>,
    /// Events on the form or report itself (control events sit on their control).
    pub events: Vec<EventDescription>,
    pub code_behind: bool,
    /// Property values that did not decode, and parser warnings.
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SourceDescription {
    /// `unbound`, `table`, `query`, `sql`, `missing`, `ambiguous`, or `named` (no package to resolve against).
    pub classification: String,
    pub raw: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SectionDescription {
    pub name: String,
    pub kind: SectionKind,
    pub visible: bool,
    pub controls: Vec<ControlDescription>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ControlDescription {
    pub name: String,
    pub kind: ControlKind,
    /// Nesting inside tab controls and pages (0 = directly in the section).
    pub depth: usize,
    pub caption: Option<String>,
    /// Caption of the attached label, when the control has one.
    pub label: Option<String>,
    pub control_source: Option<String>,
    pub default_value: Option<String>,
    pub visible: bool,
    pub enabled: bool,
    pub locked: bool,
    pub column_hidden: bool,
    pub format: Option<String>,
    pub decimal_places: Option<String>,
    pub lookup: Option<LookupDescription>,
    pub subform: Option<SubformDescription>,
    pub events: Vec<EventDescription>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LookupDescription {
    /// `values`, `sql`, `named`, `field-list`, `callback`, `empty`.
    pub kind: String,
    /// The display column of a value list.
    pub values: Vec<String>,
    pub sql: Option<String>,
    pub named: Option<String>,
    pub column_count: usize,
    /// 1-based bound column, or `row index`.
    pub bound_column: String,
    /// 1-based first visible column.
    pub display_column: Option<usize>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SubformDescription {
    pub target: String,
    /// `resolved`, `missing`, `ambiguous`, or `unresolved` (no package).
    pub resolution: String,
    /// `(master field, child field)` pairs.
    pub links: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GroupDescription {
    pub index: usize,
    pub control_source: String,
    pub header: bool,
    pub footer: bool,
    pub group_on: String,
    pub group_interval: i32,
    pub sort: String,
    pub keep_together: String,
    pub header_section: Option<String>,
    pub footer_section: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct EventDescription {
    pub event: String,
    /// `[Embedded Macro]`, `[Event Procedure]`, or a macro name.
    pub handler: String,
    /// One line per action for embedded macros.
    pub actions: Vec<String>,
}

/// One action as a line: `[condition] Action(arg, arg)`.
pub fn macro_lines(m: &Macro) -> Vec<String> {
    m.actions
        .iter()
        .map(|a| {
            let cond = a
                .condition
                .as_deref()
                .map(|c| format!("[{c}] "))
                .unwrap_or_default();
            let args: Vec<&str> = a
                .arguments
                .iter()
                .map(String::as_str)
                .filter(|s| !s.is_empty())
                .collect();
            format!("{cond}{}({})", a.action, args.join(", "))
        })
        .collect()
}

/// Events with a handler, plus embedded macros whose event property the document does not
/// list (the `Begin ... EmMacro` block is the only trace of some of them).
fn events_for(
    owner: &str,
    design: &Design,
    macros: &BTreeMap<String, Macro>,
) -> Vec<EventDescription> {
    let mut out: Vec<EventDescription> = design
        .events()
        .into_iter()
        .filter(|e| e.owner == owner)
        .map(|e| EventDescription {
            actions: macros
                .get(&format!("{}.{}", e.owner, e.event))
                .map(macro_lines)
                .unwrap_or_default(),
            event: e.event,
            handler: e.value,
        })
        .collect();
    let prefix = format!("{owner}.");
    for (key, m) in macros {
        if let Some(event) = key.strip_prefix(&prefix)
            && !out.iter().any(|e| e.event == event)
        {
            out.push(EventDescription {
                event: event.to_string(),
                handler: "[Embedded Macro]".into(),
                actions: macro_lines(m),
            });
        }
    }
    out
}

impl DesignDescription {
    /// Describe a design; with a package, record and subform sources are resolved against
    /// its object index. Attached labels and `_LayoutLabel` placeholders are folded into
    /// their control unless `all_controls` is set.
    pub fn new(design: &Design, package: Option<&Package>, all_controls: bool) -> Self {
        let mut diagnostics: Vec<String> = design.document().warnings.clone();
        let default_view = match design.default_view() {
            Ok(v) => v.map(|r| format!("{:?}", r.value).to_lowercase()),
            Err(e) => {
                diagnostics.push(e.to_string());
                design.get("DefaultView").map(String::from)
            }
        };
        let rs = design.record_source();
        let record_source = match package {
            Some(pkg) => {
                let (classification, raw) = match pkg.resolve_record_source(rs) {
                    ResolvedRecordSource::Unbound => ("unbound", String::new()),
                    ResolvedRecordSource::Table(o) => ("table", o.name.clone()),
                    ResolvedRecordSource::Query(o) => ("query", o.name.clone()),
                    ResolvedRecordSource::Sql(s) => ("sql", s.to_string()),
                    ResolvedRecordSource::Missing(s) => ("missing", s.to_string()),
                    ResolvedRecordSource::Ambiguous(_) => ("ambiguous", rs.raw().to_string()),
                };
                SourceDescription {
                    classification: classification.into(),
                    raw,
                }
            }
            None => SourceDescription {
                classification: match rs {
                    crate::RecordSource::Unbound => "unbound",
                    crate::RecordSource::Named(_) => "named",
                    crate::RecordSource::Sql(_) => "sql",
                }
                .into(),
                raw: rs.raw().to_string(),
            },
        };
        let macros = design.embedded_macros();
        let controls = design.controls();
        // Attached label captions, keyed by the control they belong to.
        let mut labels: BTreeMap<usize, String> = BTreeMap::new();
        for c in &controls {
            if c.is_attached_label()
                && let Some(DesignItem::Control(owner)) = c.parent()
                && let Some(cap) = c.raw_property("Caption")
            {
                labels.insert(owner.id().index(), cap.to_string());
            }
        }
        let mut sections: Vec<SectionDescription> = design
            .sections()
            .into_iter()
            .map(|s| SectionDescription {
                name: s.name().to_string(),
                kind: s.kind(),
                visible: s.raw_node().get("Visible") != Some("0"),
                controls: Vec::new(),
            })
            .collect();
        let mut orphans = Vec::new();
        for c in &controls {
            if !all_controls && (c.is_attached_label() || c.is_layout_label_candidate()) {
                continue;
            }
            let desc = describe_control(*c, design, package, &labels, &macros, &mut diagnostics);
            match c.section().map(|s| s.name().to_string()) {
                Some(name) => match sections.iter_mut().find(|s| s.name == name) {
                    Some(s) => s.controls.push(desc),
                    None => orphans.push(desc),
                },
                None => orphans.push(desc),
            }
        }
        if !orphans.is_empty() {
            sections.push(SectionDescription {
                name: String::new(),
                kind: SectionKind::Unknown("(no section)".into()),
                visible: true,
                controls: orphans,
            });
        }
        let groups = match design.group_levels() {
            Ok(levels) => levels.iter().map(describe_group).collect(),
            Err(e) => {
                diagnostics.push(e.to_string());
                Vec::new()
            }
        };
        let owner = match design.kind() {
            DesignKind::Form => "Form",
            DesignKind::Report => "Report",
        };
        DesignDescription {
            kind: design.kind(),
            name: design.name().to_string(),
            default_view,
            record_source,
            sections,
            groups,
            events: events_for(owner, design, &macros),
            code_behind: design.code_behind().is_some(),
            diagnostics,
        }
    }
}

fn describe_group(g: &GroupLevel<'_>) -> GroupDescription {
    GroupDescription {
        index: g.index,
        control_source: g.control_source.to_string(),
        header: g.header,
        footer: g.footer,
        group_on: format!("{:?}", g.group_on),
        group_interval: g.group_interval,
        sort: format!("{:?}", g.sort).to_lowercase(),
        keep_together: format!("{:?}", g.keep_together),
        header_section: g.header_section.map(|s| s.name().to_string()),
        footer_section: g.footer_section.map(|s| s.name().to_string()),
    }
}

fn describe_control(
    c: Control<'_>,
    design: &Design,
    package: Option<&Package>,
    labels: &BTreeMap<usize, String>,
    macros: &BTreeMap<String, Macro>,
    diagnostics: &mut Vec<String>,
) -> ControlDescription {
    let mut flag = |r: crate::PropertyResult<bool>, default: bool| match r {
        Ok(v) => v.map(|r| r.value).unwrap_or(default),
        Err(e) => {
            diagnostics.push(e.to_string());
            default
        }
    };
    let visible = flag(c.is_visible(), true);
    let enabled = flag(c.is_enabled(), true);
    let locked = flag(c.is_locked(), false);
    let column_hidden = flag(c.column_hidden(), false);
    let mut depth = 0;
    let mut p = c.parent();
    while let Some(DesignItem::Control(parent)) = p {
        depth += 1;
        p = parent.parent();
    }
    let lookup = match c.lookup() {
        Ok(l) => l.map(|l| LookupDescription {
            kind: match &l.source {
                RowSource::Empty => "empty",
                RowSource::Values(_) => "values",
                RowSource::Sql(_) => "sql",
                RowSource::Named(_) => "named",
                RowSource::FieldList(_) => "field-list",
                RowSource::Callback { .. } => "callback",
            }
            .into(),
            values: match &l.source {
                RowSource::Values(v) => {
                    let col = l.display_column().unwrap_or(0);
                    v.rows
                        .iter()
                        .map(|r| r.get(col).cloned().unwrap_or_default())
                        .collect()
                }
                _ => Vec::new(),
            },
            sql: match &l.source {
                RowSource::Sql(s) => Some(s.to_string()),
                _ => None,
            },
            named: match &l.source {
                RowSource::Named(s) | RowSource::FieldList(s) => Some(s.to_string()),
                _ => None,
            },
            column_count: l.column_count,
            bound_column: match l.bound {
                BoundColumn::RowIndex => "row index".into(),
                BoundColumn::Column(i) => (i + 1).to_string(),
            },
            display_column: l.display_column().map(|i| i + 1).or_else(|| {
                l.widths
                    .iter()
                    .all(|w| matches!(w, ColumnWidth::Auto))
                    .then_some(1)
            }),
        }),
        Err(e) => {
            diagnostics.push(e.to_string());
            None
        }
    };
    let subform = match c.subform_link() {
        Ok(l) => l.map(|l| SubformDescription {
            target: c.raw_property("SourceObject").unwrap_or("").to_string(),
            resolution: match package.map(|p| p.resolve_embedded_source(l.source)) {
                None => "unresolved",
                Some(ResolvedObject::Found(_)) => "resolved",
                Some(ResolvedObject::Empty) => "empty",
                Some(ResolvedObject::Missing(_)) => "missing",
                Some(ResolvedObject::Ambiguous(_)) => "ambiguous",
            }
            .into(),
            links: l
                .fields
                .iter()
                .map(|f| (f.master.to_string(), f.child.to_string()))
                .collect(),
        }),
        Err(e) => {
            diagnostics.push(e.to_string());
            None
        }
    };
    ControlDescription {
        name: c.name().to_string(),
        kind: c.kind(),
        depth,
        caption: c.raw_property("Caption").map(String::from),
        label: labels.get(&c.id().index()).cloned(),
        control_source: c.raw_property("ControlSource").map(String::from),
        default_value: c.raw_property("DefaultValue").map(String::from),
        visible,
        enabled,
        locked,
        column_hidden,
        format: c.format().map(|f| match f {
            DisplayFormat::Named(n) => format!("{n:?}"),
            DisplayFormat::Custom(s) => s.to_string(),
        }),
        decimal_places: match c.decimal_places() {
            Ok(v) => v.map(|r| format!("{:?}", r.value)),
            Err(e) => {
                diagnostics.push(e.to_string());
                None
            }
        },
        lookup,
        subform,
        events: events_for(c.name(), design, macros),
    }
}

impl fmt::Display for DesignDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            DesignKind::Form => "form",
            DesignKind::Report => "report",
        };
        write!(f, "{kind} {}", self.name)?;
        if let Some(v) = &self.default_view {
            write!(f, " — {v}")?;
        }
        writeln!(f)?;
        writeln!(
            f,
            "record source: {}{}",
            self.record_source.classification,
            if self.record_source.raw.is_empty() {
                String::new()
            } else {
                format!(" {}", self.record_source.raw)
            }
        )?;
        if self.code_behind {
            writeln!(f, "code behind: yes")?;
        }
        for e in &self.events {
            write_event(f, "", e)?;
        }
        for g in &self.groups {
            let mut bits = vec![format!("{} {}", g.group_on, g.sort)];
            if g.group_interval != 1 {
                bits.push(format!("interval {}", g.group_interval));
            }
            if let Some(h) = &g.header_section {
                bits.push(format!("header {h}"));
            }
            if let Some(ft) = &g.footer_section {
                bits.push(format!("footer {ft}"));
            }
            if g.keep_together != "None" {
                bits.push(format!("keep {}", g.keep_together));
            }
            writeln!(
                f,
                "{} {}: {}; {}",
                if g.header || g.footer {
                    "group"
                } else {
                    "sort"
                },
                g.index,
                g.control_source,
                bits.join("; ")
            )?;
        }
        for s in &self.sections {
            writeln!(
                f,
                "\n[{}{}]{}",
                s.name,
                if s.name.is_empty() {
                    "no section".to_string()
                } else {
                    String::new()
                },
                if s.visible { "" } else { " hidden" }
            )?;
            for c in &s.controls {
                let indent = "  ".repeat(c.depth + 1);
                let mut line = format!("{indent}{:?} {}", c.kind, c.name);
                if let Some(cap) = c.caption.as_ref().or(c.label.as_ref()) {
                    line.push_str(&format!(" \"{cap}\""));
                }
                if let Some(src) = &c.control_source {
                    line.push_str(&format!(" = {src}"));
                }
                if let Some(d) = &c.default_value {
                    line.push_str(&format!(" default {d}"));
                }
                if let Some(fm) = &c.format {
                    line.push_str(&format!(" [{fm}]"));
                }
                let mut state = Vec::new();
                if !c.visible {
                    state.push("hidden");
                }
                if !c.enabled {
                    state.push("disabled");
                }
                if c.locked {
                    state.push("locked");
                }
                if c.column_hidden {
                    state.push("column hidden");
                }
                if !state.is_empty() {
                    line.push_str(&format!(" ({})", state.join(", ")));
                }
                writeln!(f, "{line}")?;
                if let Some(l) = &c.lookup {
                    match l.kind.as_str() {
                        "values" => writeln!(f, "{indent}  values: {}", l.values.join(" | "))?,
                        "sql" => writeln!(f, "{indent}  rows: {}", l.sql.as_deref().unwrap_or(""))?,
                        _ => writeln!(
                            f,
                            "{indent}  rows ({}): {}",
                            l.kind,
                            l.named.as_deref().unwrap_or("")
                        )?,
                    }
                    if l.column_count > 1 {
                        writeln!(
                            f,
                            "{indent}  bound column {}, shows column {}",
                            l.bound_column,
                            l.display_column
                                .map(|d| d.to_string())
                                .unwrap_or_else(|| "?".into())
                        )?;
                    }
                }
                if let Some(s) = &c.subform {
                    writeln!(f, "{indent}  -> {} ({})", s.target, s.resolution)?;
                    for (m, ch) in &s.links {
                        writeln!(f, "{indent}  link: master {m} -> child {ch}")?;
                    }
                }
                for e in &c.events {
                    write_event(f, &format!("{indent}  "), e)?;
                }
            }
        }
        if !self.diagnostics.is_empty() {
            writeln!(f, "\ndiagnostics:")?;
            for d in &self.diagnostics {
                writeln!(f, "  {d}")?;
            }
        }
        Ok(())
    }
}

fn write_event(f: &mut fmt::Formatter<'_>, indent: &str, e: &EventDescription) -> fmt::Result {
    if e.actions.is_empty() {
        writeln!(f, "{indent}on {}: {}", e.event, e.handler)
    } else {
        writeln!(f, "{indent}on {}: {}", e.event, e.actions.join("; "))
    }
}

// ---- tables ----------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct TableDescription {
    pub name: String,
    pub columns: Vec<ColumnDescription>,
    pub primary_key: Vec<String>,
    pub indexes: Vec<String>,
    /// `child.column -> parent.column` for relationships that touch this table.
    pub relationships: Vec<String>,
    pub sample_rows: usize,
    pub has_data_part: bool,
    /// `None` for a plain local table; otherwise whether it is linked and its list template id.
    pub sharepoint: Option<SharePointDescription>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SharePointDescription {
    pub linked: bool,
    pub template_id: Option<u32>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ColumnDescription {
    pub name: String,
    pub jet_type: String,
    pub xsd_type: Option<String>,
    pub required: bool,
    pub auto_increment: bool,
    pub max_length: Option<u32>,
    pub default_value: Option<String>,
    pub validation_rule: Option<String>,
    pub format: Option<String>,
    /// Lookup on the field itself (`DisplayControl` combo/list with a row source).
    pub lookup: Option<LookupDescription>,
    pub description: Option<String>,
}

impl TableDescription {
    pub fn new(table: &Table, package: Option<&Package>) -> Self {
        let relationships = package
            .and_then(|p| p.relationships().ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|r| {
                r.table.eq_ignore_ascii_case(&table.name)
                    || r.referenced_table.eq_ignore_ascii_case(&table.name)
            })
            .map(|r| {
                let pairs: Vec<String> = r
                    .columns
                    .iter()
                    .map(|(c, rc)| format!("{}.{c} -> {}.{rc}", r.table, r.referenced_table))
                    .collect();
                format!(
                    "{}{}",
                    pairs.join(", "),
                    if r.enforces_integrity() {
                        ""
                    } else {
                        " (not enforced)"
                    }
                )
            })
            .collect();
        TableDescription {
            name: table.name.clone(),
            columns: table
                .columns
                .iter()
                .map(|c| ColumnDescription {
                    name: c.name.clone(),
                    jet_type: format!("{:?}", c.jet_type),
                    xsd_type: c.xsd_type.as_ref().map(|x| x.local.clone()),
                    required: c.required,
                    auto_increment: c.auto_increment,
                    max_length: c.max_length,
                    default_value: c.property("DefaultValue").map(String::from),
                    validation_rule: c.property("ValidationRule").map(String::from),
                    format: c.property("Format").map(String::from),
                    lookup: column_lookup(c),
                    description: c.description().map(String::from),
                })
                .collect(),
            primary_key: table
                .primary_key()
                .map(|i| i.columns.clone())
                .unwrap_or_default(),
            indexes: table
                .indexes
                .iter()
                .filter(|i| !i.primary)
                .map(|i| {
                    format!(
                        "{}{} ({})",
                        i.name,
                        if i.unique { " unique" } else { "" },
                        i.columns.join(", ")
                    )
                })
                .collect(),
            relationships,
            sample_rows: table.rows.len(),
            has_data_part: table.has_data_part,
            sharepoint: table
                .sharepoint_metadata
                .as_ref()
                .map(|sp| SharePointDescription {
                    linked: table.is_linked(),
                    template_id: sp.template_id,
                }),
        }
    }
}

/// A field-level lookup: `RowSourceType` + `RowSource` field properties, shown by a combo or
/// list box (`DisplayControl` 111 / 110).
fn column_lookup(c: &crate::Column) -> Option<LookupDescription> {
    let display = c.property("DisplayControl")?;
    if !matches!(display, "110" | "111") {
        return None;
    }
    let raw = c.property("RowSource").unwrap_or("");
    let kind = c.property("RowSourceType").unwrap_or("");
    let values: Vec<String> = if kind.eq_ignore_ascii_case("Value List") {
        raw.split(';')
            .map(|v| v.trim().trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
            .collect()
    } else {
        Vec::new()
    };
    let column_count: usize = c
        .property("ColumnCount")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let bound: usize = c
        .property("BoundColumn")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let widths: Vec<i64> = c
        .property("ColumnWidths")
        .map(|w| w.split(';').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_default();
    Some(LookupDescription {
        kind: if kind.eq_ignore_ascii_case("Value List") {
            "values"
        } else if raw.trim_start().to_ascii_uppercase().starts_with("SELECT") {
            "sql"
        } else if raw.is_empty() {
            "empty"
        } else {
            "named"
        }
        .into(),
        values,
        sql: (!raw.is_empty() && !kind.eq_ignore_ascii_case("Value List")).then(|| raw.to_string()),
        named: None,
        column_count,
        bound_column: bound.to_string(),
        // Widths are listed from the first column; an unlisted column has its default width.
        display_column: widths
            .iter()
            .position(|w| *w != 0)
            .map(|i| i + 1)
            .or_else(|| (column_count > widths.len()).then_some(widths.len() + 1))
            .or(Some(1)),
    })
}

impl fmt::Display for TableDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "table {}", self.name)?;
        match &self.sharepoint {
            Some(sp) if sp.linked => write!(
                f,
                " — linked SharePoint list (template {}; rows not included)",
                sp.template_id
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "unknown".into())
            )?,
            Some(sp) => write!(
                f,
                " — local (publishable as list template {})",
                sp.template_id
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "unknown".into())
            )?,
            None => {}
        }
        writeln!(f)?;
        writeln!(
            f,
            "sample rows: {}{}",
            self.sample_rows,
            if self.has_data_part {
                ""
            } else {
                " (no data part)"
            }
        )?;
        if !self.primary_key.is_empty() {
            writeln!(f, "primary key: {}", self.primary_key.join(", "))?;
        }
        for c in &self.columns {
            let mut line = format!("  {} {}", c.name, c.jet_type);
            if let Some(n) = c.max_length {
                line.push_str(&format!("({n})"));
            }
            if c.auto_increment {
                line.push_str(" autonumber");
            }
            if c.required {
                line.push_str(" required");
            }
            if let Some(d) = &c.default_value {
                line.push_str(&format!(" default {d}"));
            }
            if let Some(v) = &c.validation_rule {
                line.push_str(&format!(" rule {v}"));
            }
            if let Some(fm) = &c.format {
                line.push_str(&format!(" [{fm}]"));
            }
            if let Some(d) = &c.description {
                line.push_str(&format!(" \"{d}\""));
            }
            writeln!(f, "{line}")?;
            if let Some(l) = &c.lookup {
                if l.kind == "values" {
                    writeln!(f, "    values: {}", l.values.join(" | "))?;
                } else if let Some(sql) = &l.sql {
                    writeln!(f, "    rows: {sql}")?;
                }
                if l.column_count > 1 {
                    writeln!(
                        f,
                        "    bound column {}, shows column {}",
                        l.bound_column,
                        l.display_column.unwrap_or(1)
                    )?;
                }
            }
        }
        for i in &self.indexes {
            writeln!(f, "index: {i}")?;
        }
        for r in &self.relationships {
            writeln!(f, "relationship: {r}")?;
        }
        Ok(())
    }
}
