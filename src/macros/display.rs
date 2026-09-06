//! One-line renderings of macro steps and actions, in Access's own vocabulary.

use std::fmt;

use super::{Action, Macro, Step, Submacro};

fn field<T: fmt::Debug>(out: &mut Vec<String>, key: &str, value: &T) {
    out.push(format!("{key}={value:?}"));
}

fn opt<T: fmt::Display>(out: &mut Vec<String>, key: &str, value: &Option<T>) {
    if let Some(v) = value {
        out.push(format!("{key}={v}"));
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        let name = match self {
            Action::OpenForm {
                form,
                view,
                filter_name,
                where_condition,
                data_mode,
                window_mode,
            } => {
                parts.push(form.clone());
                field(&mut parts, "view", view);
                opt(&mut parts, "filter", filter_name);
                opt(&mut parts, "where", where_condition);
                field(&mut parts, "data", data_mode);
                field(&mut parts, "window", window_mode);
                "OpenForm"
            }
            Action::OpenReport {
                report,
                view,
                filter_name,
                where_condition,
                window_mode,
            } => {
                parts.push(report.clone());
                field(&mut parts, "view", view);
                opt(&mut parts, "filter", filter_name);
                opt(&mut parts, "where", where_condition);
                field(&mut parts, "window", window_mode);
                "OpenReport"
            }
            Action::OpenQuery {
                query,
                view,
                data_mode,
            } => {
                parts.push(query.clone());
                field(&mut parts, "view", view);
                field(&mut parts, "data", data_mode);
                "OpenQuery"
            }
            Action::OpenTable {
                table,
                view,
                data_mode,
            } => {
                parts.push(table.clone());
                field(&mut parts, "view", view);
                field(&mut parts, "data", data_mode);
                "OpenTable"
            }
            Action::RunCommand(cmd) => {
                parts.push(format!("{cmd:?}"));
                "RunCommand"
            }
            Action::RunMacro { name } => {
                parts.push(name.clone());
                "RunMacro"
            }
            Action::SetTempVar { name, value } => {
                parts.push(format!("{name} = {value}"));
                "SetTempVar"
            }
            Action::RemoveTempVar { name } => {
                parts.push(name.clone());
                "RemoveTempVar"
            }
            Action::ApplyFilter {
                filter_name,
                where_condition,
            } => {
                opt(&mut parts, "filter", filter_name);
                opt(&mut parts, "where", where_condition);
                "ApplyFilter"
            }
            Action::SetValue { item, expr } => {
                parts.push(format!("{item} = {expr}"));
                "SetValue"
            }
            Action::SetProperty {
                control,
                property,
                value,
            } => {
                parts.push(format!("{control}.{property:?} = {value}"));
                "SetProperty"
            }
            Action::SetWarnings { on } => {
                parts.push(if *on { "on".into() } else { "off".into() });
                "SetWarnings"
            }
            Action::MsgBox {
                message,
                beep,
                type_,
                title,
            } => {
                parts.push(message.to_string());
                if *beep {
                    parts.push("beep".into());
                }
                field(&mut parts, "type", type_);
                opt(&mut parts, "title", title);
                "MsgBox"
            }
            Action::SendObject {
                object_type,
                object_name,
                format,
                to,
                cc,
                bcc,
                subject,
                message,
                edit_message,
            } => {
                field(&mut parts, "object", object_type);
                opt(&mut parts, "name", object_name);
                opt(&mut parts, "format", format);
                opt(&mut parts, "to", to);
                opt(&mut parts, "cc", cc);
                opt(&mut parts, "bcc", bcc);
                opt(&mut parts, "subject", subject);
                opt(&mut parts, "message", message);
                if *edit_message {
                    parts.push("edit".into());
                }
                "SendObject"
            }
            Action::Close {
                object_type,
                object_name,
                save,
            } => {
                field(&mut parts, "object", object_type);
                opt(&mut parts, "name", object_name);
                field(&mut parts, "save", save);
                "Close"
            }
            Action::Requery { control } => {
                if let Some(c) = control {
                    parts.push(c.trim_start_matches('=').to_string());
                }
                "Requery"
            }
            Action::GoToRecord {
                object_type,
                object_name,
                record,
                offset,
            } => {
                field(&mut parts, "object", object_type);
                opt(&mut parts, "name", object_name);
                field(&mut parts, "record", record);
                opt(&mut parts, "offset", offset);
                "GoToRecord"
            }
            Action::GoToControl { control } => {
                parts.push(control.clone());
                "GoToControl"
            }
            Action::SearchForRecord {
                object_type,
                object_name,
                record,
                where_condition,
            } => {
                field(&mut parts, "object", object_type);
                opt(&mut parts, "name", object_name);
                field(&mut parts, "record", record);
                opt(&mut parts, "where", where_condition);
                "SearchForRecord"
            }
            Action::MoveSize {
                right,
                down,
                width,
                height,
            } => {
                opt(&mut parts, "right", right);
                opt(&mut parts, "down", down);
                opt(&mut parts, "width", width);
                opt(&mut parts, "height", height);
                "MoveSize"
            }
            Action::Beep => "Beep",
            Action::StopMacro => "StopMacro",
            Action::CancelEvent => "CancelEvent",
            Action::OnError { next, macro_name } => {
                field(&mut parts, "next", next);
                opt(&mut parts, "macro", macro_name);
                "OnError"
            }
            Action::ClearMacroError => "ClearMacroError",
            Action::Comment(text) => {
                parts.push(text.clone());
                "Comment"
            }
            Action::Unknown { name, arguments } => {
                parts.extend(arguments.iter().filter(|a| !a.is_empty()).cloned());
                return write!(f, "{name}({})", parts.join(", "));
            }
        };
        if parts.is_empty() {
            f.write_str(name)
        } else {
            write!(f, "{name}({})", parts.join(", "))
        }
    }
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Step::Always(a) => write!(f, "{a}"),
            Step::When { cond, body } => write!(
                f,
                "If {cond}: {}",
                body.iter()
                    .map(|a| a.to_string())
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        }
    }
}

impl fmt::Display for Submacro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(n) = &self.name {
            writeln!(f, "{n}:")?;
        }
        for s in &self.steps {
            writeln!(f, "  {s}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Macro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for s in &self.submacros {
            write!(f, "{s}")?;
        }
        Ok(())
    }
}
