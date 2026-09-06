//! Standalone macros (the `Macro` objects of the navigation pane).

use crate::saveastext;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct MacroAction {
    pub condition: Option<String>,
    pub action: String,
    pub arguments: Vec<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Macro {
    pub name: String,
    pub version: Option<String>,
    pub actions: Vec<MacroAction>,
    pub text: String,
}

impl Macro {
    /// Parse a macro from SaveAsText text.
    pub fn parse(name: &str, text: String) -> Macro {
        let doc = saveastext::parse(&text);
        let actions = doc
            .blocks
            .iter()
            .filter(|b| b.get("Action").is_some())
            .map(|b| MacroAction {
                condition: b.get("Condition").map(String::from),
                action: b.get("Action").unwrap_or("").to_string(),
                arguments: b.values("Argument").into_iter().map(String::from).collect(),
                comment: b.get("Comment").map(String::from),
            })
            .collect();
        Macro { name: name.to_string(), version: doc.get("Version").map(String::from), actions, text }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_actions() {
        let m = Macro::parse("AutoExec", "Version =196611\nBegin\n    Condition =\"Not [CurrentProject].[IsTrusted]\"\n    Action =\"OpenForm\"\n    Argument =\"frmStartup\"\n    Argument =\"0\"\nEnd\nBegin\n    Action =\"RunCode\"\n    Argument =\"Startup()\"\nEnd\n".to_string());
        assert_eq!(m.actions.len(), 2);
        assert_eq!(m.actions[0].action, "OpenForm");
        assert_eq!(m.actions[0].arguments, vec!["frmStartup", "0"]);
        assert_eq!(m.actions[0].condition.as_deref(), Some("Not [CurrentProject].[IsTrusted]"));
    }
}
