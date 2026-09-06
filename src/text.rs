//! Text decoding and small XML helpers shared by the parsers.

/// Decode a part's bytes by BOM: UTF-16LE, UTF-16BE, UTF-8 with BOM, or UTF-8. Lossy: invalid
/// sequences and a dangling trailing byte become U+FFFD.
pub fn decode(bytes: &[u8]) -> String {
    let utf16 = |body: &[u8], from: fn([u8; 2]) -> u16| {
        let mut units: Vec<u16> = body.chunks_exact(2).map(|c| from([c[0], c[1]])).collect();
        if !body.len().is_multiple_of(2) {
            units.push(0xFFFD);
        }
        String::from_utf16_lossy(&units)
    };
    if bytes.starts_with(&[0xFF, 0xFE]) {
        utf16(&bytes[2..], u16::from_le_bytes)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        utf16(&bytes[2..], u16::from_be_bytes)
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Decode an XML part and normalise its declaration so the XML parser accepts it.
pub fn decode_xml(bytes: &[u8]) -> String {
    let text = decode(bytes);
    text.replacen("encoding=\"UTF-16\"", "encoding=\"UTF-8\"", 1)
        .replacen("encoding=\"utf-16\"", "encoding=\"UTF-8\"", 1)
        .replacen("encoding=\"unicode\"", "encoding=\"UTF-8\"", 1)
}

/// Undo the `_xHHHH_` escaping Access uses for XML names ("Order_x0020_Details").
pub fn unescape_xml_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(pos) = rest.find("_x") {
        let (head, tail) = rest.split_at(pos);
        out.push_str(head);
        // `_xHHHH_` is seven ASCII bytes; check bytes so multi-byte characters never get sliced.
        let b = tail.as_bytes();
        let is_escape = b.len() >= 7 && b[2..6].iter().all(u8::is_ascii_hexdigit) && b[6] == b'_';
        let code = is_escape
            .then(|| u32::from_str_radix(&tail[2..6], 16).ok())
            .flatten()
            .and_then(char::from_u32);
        if let Some(c) = code {
            out.push(c);
            rest = &tail[7..];
            continue;
        }
        out.push_str("_x");
        rest = &tail[2..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn parse_xml<'a>(part: &str, text: &'a str) -> crate::Result<roxmltree::Document<'a>> {
    roxmltree::Document::parse(text).map_err(|source| crate::Error::Xml {
        part: part.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_boms() {
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'h', b'i']), "hi");
        assert_eq!(decode(&[0xFF, 0xFE, b'h', 0, b'i', 0]), "hi");
        assert_eq!(decode(b"hi"), "hi");
        assert_eq!(decode(&[0xFE, 0xFF, 0, b'h', 0, b'i']), "hi");
        assert_eq!(decode(&[0xFF, 0xFE, b'A', 0, 0x42]), "A\u{FFFD}");
    }

    #[test]
    fn unescapes_names() {
        assert_eq!(unescape_xml_name("Order_x0020_Details"), "Order Details");
        assert_eq!(unescape_xml_name("Plain"), "Plain");
        assert_eq!(unescape_xml_name("_x0031_st"), "1st");
        assert_eq!(unescape_xml_name("_xééé_"), "_xééé_");
        assert_eq!(unescape_xml_name("a_x"), "a_x");
    }
}
