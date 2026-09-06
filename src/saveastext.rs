//! Parser for Access's `SaveAsText` format, which templates use for forms, reports, macros
//! and queries.
//!
//! The grammar is line based: `Key =Value`, `Key = Begin` … `End` (a multi-line value,
//! usually hex), `Begin <Kind>` … `End` (a nested block), a bare `Begin` (an anonymous
//! block: a macro action, or the wrapper around a section's controls), and, for forms and
//! reports, a `CodeBehindForm` line after which the rest of the file is VBA.

use std::collections::BTreeMap;

/// A block: `Begin <kind>` … `End`. Anonymous blocks have the kind `"Block"`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Node {
    pub kind: String,
    /// Last value per key (repeated keys such as a macro's `Argument` lines get `#n` suffixes).
    pub properties: BTreeMap<String, String>,
    /// Every `Key =Value` line in file order, with the original key.
    pub entries: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    /// All values for a key, in order (`Argument`, `Expression`, …).
    pub fn values(&self, key: &str) -> Vec<&str> {
        self.entries.iter().filter(|(k, _)| k == key).map(|(_, v)| v.as_str()).collect()
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Document {
    /// `Key =Value` lines outside any block (`Version`, `Operation`, `Where`, …).
    pub header: BTreeMap<String, String>,
    pub header_entries: Vec<(String, String)>,
    pub blocks: Vec<Node>,
    /// VBA source after `CodeBehindForm`, if present.
    pub code_behind: Option<String>,
}

impl Document {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.header.get(key).map(String::as_str)
    }

    pub fn block(&self, kind: &str) -> Option<&Node> {
        self.blocks.iter().find(|b| b.kind == kind)
    }
}

pub fn parse(text: &str) -> Document {
    let mut doc = Document::default();
    let mut stack: Vec<Node> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(raw) = lines.next() {
        let t = raw.trim_end_matches('\r').trim();
        if t.is_empty() {
            continue;
        }
        if t == "CodeBehindForm" {
            let rest: Vec<&str> = lines.map(|l| l.trim_end_matches('\r')).collect();
            doc.code_behind = Some(rest.join("\n"));
            break;
        }
        if t == "End" {
            if let Some(node) = stack.pop() {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => doc.blocks.push(node),
                }
            }
            continue;
        }
        if let Some(kind) = t.strip_prefix("Begin") {
            let kind = kind.trim();
            if kind.is_empty() || kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                stack.push(Node { kind: if kind.is_empty() { "Block".into() } else { kind.into() }, ..Default::default() });
                continue;
            }
        }
        let Some((key, value)) = t.split_once('=') else { continue };
        let key = normalize_key(key.trim());
        let value = value.trim();
        let value = if value == "Begin" {
            let mut parts = Vec::new();
            for l in lines.by_ref() {
                let l = l.trim();
                if l == "End" {
                    break;
                }
                parts.push(l.trim_end_matches(',').trim().to_string());
            }
            parts.join("")
        } else {
            unquote(value)
        };
        match stack.last_mut() {
            Some(node) => {
                node.entries.push((key.clone(), value.clone()));
                if node.properties.contains_key(&key) {
                    let n = node.properties.keys().filter(|k| k.starts_with(&key)).count();
                    node.properties.insert(format!("{key}#{n}"), value);
                } else {
                    node.properties.insert(key, value);
                }
            }
            None => {
                doc.header_entries.push((key.clone(), value.clone()));
                doc.header.insert(key, value);
            }
        }
    }
    while let Some(node) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.children.push(node),
            None => doc.blocks.push(node),
        }
    }
    doc
}

/// `dbBoolean "ReturnsRecords"` -> `ReturnsRecords`.
fn normalize_key(key: &str) -> String {
    match key.find('"') {
        Some(start) => key[start + 1..].trim_end_matches('"').to_string(),
        None => key.to_string(),
    }
}

fn unquote(v: &str) -> String {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        v[1..v.len() - 1].replace("\"\"", "\"")
    } else {
        v.to_string()
    }
}

/// Decode a hex-encoded property value (`0x0a0b…`, possibly from several joined lines).
pub fn hex_bytes(v: &str) -> Option<Vec<u8>> {
    let cleaned: String = v.split("0x").collect::<Vec<_>>().join("");
    if cleaned.is_empty() || !cleaned.len().is_multiple_of(2) || !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..cleaned.len()).step_by(2).map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks_values_and_code() {
        let text = "Version =21\r\nBegin Form\r\n    Caption =\"Login\"\r\n    NameMap = Begin\r\n        0x0a ,\r\n        0x0b\r\n    End\r\n    Begin\r\n        Begin Label\r\n            FontSize =11\r\n        End\r\n    End\r\nEnd\r\nCodeBehindForm\r\nOption Explicit\r\n";
        let doc = parse(text);
        assert_eq!(doc.get("Version"), Some("21"));
        let form = doc.block("Form").unwrap();
        assert_eq!(form.get("Caption"), Some("Login"));
        assert_eq!(form.get("NameMap"), Some("0x0a0x0b"));
        assert_eq!(form.children[0].kind, "Block");
        assert_eq!(form.children[0].children[0].kind, "Label");
        assert_eq!(doc.code_behind.as_deref(), Some("Option Explicit"));
        assert_eq!(hex_bytes("0x0a0x0b"), Some(vec![0x0a, 0x0b]));
    }

    #[test]
    fn keeps_repeated_keys_in_order() {
        let doc = parse("Begin\n    Action =\"OpenForm\"\n    Argument =\"A\"\n    Argument =\"B\"\nEnd\n");
        assert_eq!(doc.blocks[0].values("Argument"), vec!["A", "B"]);
        assert_eq!(doc.blocks[0].get("Argument#1"), Some("B"));
    }

    #[test]
    fn normalizes_typed_keys() {
        let doc = parse("dbBoolean \"ReturnsRecords\" =\"-1\"\n");
        assert_eq!(doc.get("ReturnsRecords"), Some("-1"));
    }
}
