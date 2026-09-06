//! Parser for Access's `SaveAsText` format, which templates use for forms, reports, macros
//! and queries.
//!
//! The grammar is line based: `Key =Value`, `Key = Begin` … `End` (a multi-line value,
//! usually hex), `Begin <Kind>` … `End` (a nested block), a bare `Begin` (an anonymous
//! block: a macro action, or the wrapper around a section's controls), and, for forms and
//! reports, a `CodeBehindForm` line after which the rest of the file is VBA.
//!
//! Quoted string values wrap at about 100 characters onto continuation lines that hold
//! only a quoted fragment, and use backslash escapes (`\"`, `\\`, `\015`) inside quotes.
//! Both are undone here. A `Key = Begin` block holds either hex lines (binary data, stored
//! as the property's value) or nested text such as an embedded macro (stored as a child
//! block whose kind is the key).

use std::collections::BTreeMap;

/// A block: `Begin <kind>` … `End`. Anonymous blocks have the kind `"Block"`.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Node {
    pub kind: String,
    /// First value per key. Repeated keys (a macro's `Argument` lines) are in `entries`.
    pub properties: BTreeMap<String, String>,
    /// Every `Key =Value` line in file order.
    pub entries: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(String::as_str)
    }

    /// All values for a key, in order.
    pub fn values(&self, key: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Document {
    /// `Key =Value` lines outside any block (`Version`, `Operation`, `Where`, …), first value per key.
    pub header: BTreeMap<String, String>,
    pub header_entries: Vec<(String, String)>,
    pub blocks: Vec<Node>,
    /// VBA source after `CodeBehindForm`, if present.
    pub code_behind: Option<String>,
    /// Structural problems found while parsing (unmatched `End`, unterminated blocks,
    /// lines that fit no rule). The document is still usable.
    pub warnings: Vec<String>,
}

impl Document {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.header.get(key).map(String::as_str)
    }

    pub fn block(&self, kind: &str) -> Option<&Node> {
        self.blocks.iter().find(|b| b.kind == kind)
    }

    /// The document's first block (the `Form`, `Report`, or first macro action).
    pub fn root(&self) -> Option<&Node> {
        self.blocks.first()
    }
}

pub fn parse(text: &str) -> Document {
    let mut doc = Document::default();
    // Each open block, with whether it is a `Key = Begin` value block and its hex lines so far.
    let mut stack: Vec<(Node, bool, Vec<String>)> = Vec::new();
    let mut lines = text
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .enumerate()
        .peekable();
    while let Some((idx, raw)) = lines.next() {
        let line_no = idx + 1;
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        if t == "CodeBehindForm" {
            let rest: Vec<&str> = lines.map(|(_, l)| l).collect();
            doc.code_behind = Some(rest.join("\n"));
            break;
        }
        if t == "End" {
            match stack.pop() {
                Some((node, true, hex)) => {
                    // A value block: binary data becomes the property value, nested text a child block.
                    let is_binary = node.entries.is_empty() && node.children.is_empty();
                    match stack.last_mut() {
                        Some((parent, _, _)) => {
                            if is_binary {
                                let value = hex.join("");
                                parent.entries.push((node.kind.clone(), value.clone()));
                                parent.properties.entry(node.kind).or_insert(value);
                            } else {
                                parent.children.push(node);
                            }
                        }
                        None => {
                            if is_binary {
                                let value = hex.join("");
                                doc.header_entries.push((node.kind.clone(), value.clone()));
                                doc.header.entry(node.kind).or_insert(value);
                            } else {
                                doc.blocks.push(node);
                            }
                        }
                    }
                }
                Some((node, false, _)) => match stack.last_mut() {
                    Some((parent, _, _)) => parent.children.push(node),
                    None => doc.blocks.push(node),
                },
                None => doc
                    .warnings
                    .push(format!("line {line_no}: End without a matching Begin")),
            }
            continue;
        }
        if let Some(kind) = t.strip_prefix("Begin") {
            let kind = kind.trim();
            if kind.is_empty() || kind.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                stack.push((
                    Node {
                        kind: if kind.is_empty() {
                            "Block".into()
                        } else {
                            kind.into()
                        },
                        ..Default::default()
                    },
                    false,
                    Vec::new(),
                ));
                continue;
            }
        }
        if t.starts_with("0x") {
            match stack.last_mut() {
                Some((_, true, hex)) => hex.push(t.trim_end_matches(',').trim().to_string()),
                _ => doc
                    .warnings
                    .push(format!("line {line_no}: hex data outside a value block")),
            }
            continue;
        }
        let Some((key, value)) = t.split_once('=') else {
            doc.warnings
                .push(format!("line {line_no}: unrecognised line {t:?}"));
            continue;
        };
        let key = normalize_key(key.trim());
        let value = value.trim();
        if value == "Begin" {
            stack.push((
                Node {
                    kind: key,
                    ..Default::default()
                },
                true,
                Vec::new(),
            ));
            continue;
        }
        let value = if let Some(inner) = quoted(value) {
            let mut s = inner.to_string();
            while let Some((_, next)) = lines.peek() {
                match quoted(next.trim()) {
                    Some(fragment) => {
                        s.push_str(fragment);
                        lines.next();
                    }
                    None => break,
                }
            }
            unescape(&s)
        } else {
            value.to_string()
        };
        match stack.last_mut() {
            Some((node, _, _)) => {
                node.entries.push((key.clone(), value.clone()));
                node.properties.entry(key).or_insert(value);
            }
            None => {
                doc.header_entries.push((key.clone(), value.clone()));
                doc.header.entry(key).or_insert(value);
            }
        }
    }
    while let Some((node, _, _)) = stack.pop() {
        doc.warnings
            .push(format!("unterminated block {:?}", node.kind));
        match stack.last_mut() {
            Some((parent, _, _)) => parent.children.push(node),
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

/// The inside of a `"…"` token, or None when the text is not a quoted string.
fn quoted(v: &str) -> Option<&str> {
    (v.len() >= 2 && v.starts_with('"') && v.ends_with('"')).then(|| &v[1..v.len() - 1])
}

/// Undo SaveAsText's backslash escapes: `\"`, `\\`, and `\NNN` octal bytes (`\015\012`).
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('"') => {
                out.push('"');
                chars.next();
            }
            Some('\\') => {
                out.push('\\');
                chars.next();
            }
            Some(d) if d.is_digit(8) => {
                let mut code = 0u32;
                let mut n = 0;
                while n < 3 {
                    match chars.peek().and_then(|c| c.to_digit(8)) {
                        Some(v) => {
                            code = code * 8 + v;
                            chars.next();
                            n += 1;
                        }
                        None => break,
                    }
                }
                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
            }
            _ => out.push('\\'),
        }
    }
    out
}

/// Decode a hex-encoded property value (`0x0a0b…`, possibly from several joined lines).
pub fn hex_bytes(v: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = v
        .replace("0x", "")
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b',')
        .collect();
    if digits.is_empty()
        || !digits.len().is_multiple_of(2)
        || !digits.iter().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    let hex = |b: u8| (b as char).to_digit(16).unwrap() as u8;
    let (pairs, _) = digits.as_chunks::<2>();
    Some(pairs.iter().map(|p| hex(p[0]) << 4 | hex(p[1])).collect())
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
        assert!(doc.warnings.is_empty());
    }

    #[test]
    fn joins_continuations_and_unescapes() {
        let text = "Begin Form\n    Caption =\"This form uses the \\\"new\\\" control, which is not available\"\n        \" in older versions.\\015\\012Second line \\\\ backslash\"\n    Width =100\nEnd\n";
        let doc = parse(text);
        let form = doc.block("Form").unwrap();
        assert_eq!(
            form.get("Caption"),
            Some(
                "This form uses the \"new\" control, which is not available in older versions.\r\nSecond line \\ backslash"
            )
        );
        assert_eq!(form.get("Width"), Some("100"));
    }

    #[test]
    fn value_blocks_are_hex_or_nested_text() {
        let text = "Begin Form\n    Begin CommandButton\n        Name =\"cmdGo\"\n        GUID = Begin\n            0x0102\n        End\n        OnClickEmMacro = Begin\n            Version =196611\n            Begin\n                Action =\"OpenReport\"\n                Argument =\"rptLearn\"\n            End\n            Begin\n                Comment =\"_AXL:<x/>\"\n            End\n        End\n        Width =10\n    End\nEnd\n";
        let doc = parse(text);
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
        let button = &doc.blocks[0].children[0];
        assert_eq!(button.get("GUID"), Some("0x0102"));
        assert_eq!(button.get("Width"), Some("10"));
        let em = button
            .children
            .iter()
            .find(|c| c.kind == "OnClickEmMacro")
            .unwrap();
        assert_eq!(em.get("Version"), Some("196611"));
        assert_eq!(em.children[0].get("Action"), Some("OpenReport"));
    }

    #[test]
    fn keeps_repeated_keys_in_order() {
        let doc = parse(
            "Begin\n    NameMap =\"x\"\n    Name =\"a\"\n    Name =\"b\"\n    Argument =\"A\"\n    Argument =\"B\"\nEnd\n",
        );
        assert_eq!(doc.blocks[0].values("Argument"), vec!["A", "B"]);
        assert_eq!(doc.blocks[0].values("Name"), vec!["a", "b"]);
        assert_eq!(doc.blocks[0].get("Name"), Some("a"));
        assert_eq!(doc.blocks[0].properties.len(), 3);
    }

    #[test]
    fn normalizes_typed_keys_and_reports_problems() {
        let doc = parse("dbBoolean \"ReturnsRecords\" =\"-1\"\nEnd\nBegin Form\n");
        assert_eq!(doc.get("ReturnsRecords"), Some("-1"));
        assert_eq!(doc.warnings.len(), 2, "{:?}", doc.warnings);
    }
}
