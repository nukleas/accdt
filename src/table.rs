//! Tables: schema from the object's XML Schema part, rows from its sample-data part.

use crate::text::{self, unescape_xml_name};

const XSD: &str = "http://www.w3.org/2001/XMLSchema";
const OD: &str = "urn:schemas-microsoft-com:officedata";

/// Access field types as written in `od:jetType`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum JetType {
    AutoNumber,
    Text,
    Memo,
    Byte,
    Integer,
    LongInteger,
    Single,
    Double,
    Currency,
    Decimal,
    DateTime,
    YesNo,
    OleObject,
    Hyperlink,
    /// Attachment or multi-valued field; see `Column::complex_type`.
    Complex,
    Guid,
    Other(String),
}

impl JetType {
    fn parse(v: &str) -> JetType {
        match v.to_ascii_lowercase().as_str() {
            "autonumber" => JetType::AutoNumber,
            "text" => JetType::Text,
            "memo" => JetType::Memo,
            "byte" => JetType::Byte,
            "integer" => JetType::Integer,
            "longinteger" => JetType::LongInteger,
            "single" => JetType::Single,
            "double" => JetType::Double,
            "currency" => JetType::Currency,
            "decimal" => JetType::Decimal,
            "datetime" => JetType::DateTime,
            "yesno" => JetType::YesNo,
            "oleobject" => JetType::OleObject,
            "hyperlink" => JetType::Hyperlink,
            "complex" => JetType::Complex,
            "replicationid" | "guid" => JetType::Guid,
            other => JetType::Other(other.to_string()),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Column {
    pub name: String,
    pub jet_type: JetType,
    /// `od:sqlSType` (`int`, `nvarchar`, `datetime`, `money`, …).
    pub sql_type: Option<String>,
    pub required: bool,
    pub auto_increment: bool,
    /// From `xsd:maxLength` on text columns.
    pub max_length: Option<u32>,
    /// `od:jetComplexType` for attachment/multi-valued columns.
    pub complex_type: Option<String>,
    pub xsd_type: Option<ExpandedName>,
    pub restrictions: Vec<Restriction>,
    /// Ordered nested schemas for attachment and multi-valued record children.
    pub children: Vec<Column>,
    /// `od:fieldProperty` entries: `(name, type code, value)`.
    pub properties: Vec<(String, String, String)>,
}

impl Column {
    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, _, v)| v.as_str())
    }
    pub fn lookup(&self) -> Result<Option<crate::Lookup<'_>>, crate::PropertyError> {
        let applicable = matches!(
            self.display_control()?.map(|r| r.value),
            Some(crate::ControlKind::ComboBox | crate::ControlKind::ListBox)
        ) || self.property("RowSource").is_some();
        if !applicable {
            return Ok(None);
        }
        crate::design::lookup("", &self.name, false, |key| self.property(key)).map(Some)
    }
    pub fn display_control(&self) -> crate::PropertyResult<crate::ControlKind> {
        self.property("DisplayControl")
            .map(|value| {
                crate::design::integer(
                    "",
                    &self.name,
                    "DisplayControl",
                    crate::Resolved {
                        value,
                        origin: crate::PropertyOrigin::Explicit,
                    },
                    crate::ControlKind::from_code,
                )
            })
            .transpose()
    }
    pub fn format(&self) -> Option<crate::DisplayFormat<'_>> {
        self.property("Format").map(crate::DisplayFormat::parse)
    }
    pub fn input_mask(&self) -> Option<&str> {
        self.property("InputMask")
    }
    pub fn decimal_places(&self) -> crate::PropertyResult<crate::DecimalPlaces> {
        self.property("DecimalPlaces")
            .map(|value| {
                crate::design::integer(
                    "",
                    &self.name,
                    "DecimalPlaces",
                    crate::Resolved {
                        value,
                        origin: crate::PropertyOrigin::Explicit,
                    },
                    crate::DecimalPlaces::from_code,
                )
            })
            .transpose()
    }
    pub fn description(&self) -> Option<&str> {
        self.property("Description")
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Index {
    pub name: String,
    pub columns: Vec<String>,
    pub primary: bool,
    pub unique: bool,
    /// Per-column `asc`/`desc`.
    pub order: Vec<String>,
}

/// Namespace-expanded XSD type name, independent of the document's prefix spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ExpandedName {
    pub namespace: Option<String>,
    pub local: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Restriction {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Row {
    pub cells: Vec<Cell>,
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "kind"))]
pub enum Cell {
    Null {
        encoding: NullEncoding,
    },
    Value {
        value: Value,
        lexical: Option<String>,
    },
    Invalid {
        lexical: String,
        error: ValueError,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum NullEncoding {
    Absent,
    XsiNil,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{part}: {table} row {row:?} column {column}: {reason}")]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ValueError {
    pub part: String,
    pub table: String,
    pub row: Option<usize>,
    pub column: String,
    pub reason: String,
}
impl ValueError {
    fn for_column(column: &str, reason: impl Into<String>) -> Self {
        Self {
            part: String::new(),
            table: String::new(),
            row: None,
            column: column.into(),
            reason: reason.into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(tag = "type", content = "value"))]
pub enum Value {
    Text(String),
    Boolean(bool),
    Integer(i64),
    Single(#[cfg_attr(feature = "serde", serde(serialize_with = "serialize_single"))] f32),
    Double(#[cfg_attr(feature = "serde", serde(serialize_with = "serialize_double"))] f64),
    Decimal(Decimal),
    Currency(Currency),
    DateTime(XmlDateTime),
    Binary(Vec<u8>),
    Guid([u8; 16]),
    Complex(Vec<ComplexRecord>),
    Uninterpreted {
        xsd_type: Option<ExpandedName>,
        text: String,
    },
}
#[cfg(feature = "serde")]
fn serialize_double<S: serde::Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if v.is_finite() {
        s.serialize_f64(*v)
    } else {
        s.serialize_str(if v.is_nan() {
            "NaN"
        } else if v.is_sign_positive() {
            "INF"
        } else {
            "-INF"
        })
    }
}
#[cfg(feature = "serde")]
fn serialize_single<S: serde::Serializer>(v: &f32, s: S) -> Result<S::Ok, S::Error> {
    if v.is_finite() {
        s.serialize_f32(*v)
    } else {
        s.serialize_str(if v.is_nan() {
            "NaN"
        } else if v.is_sign_positive() {
            "INF"
        } else {
            "-INF"
        })
    }
}
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ComplexRecord {
    pub fields: Vec<(String, Cell)>,
}
impl Value {
    pub fn as_str(&self) -> Option<&str> {
        if let Self::Text(s) = self {
            Some(s)
        } else {
            None
        }
    }
}
impl Cell {
    pub fn value(&self) -> Option<&Value> {
        if let Self::Value { value, .. } = self {
            Some(value)
        } else {
            None
        }
    }
    pub fn lexical(&self) -> Option<&str> {
        match self {
            Self::Value { lexical, .. } => lexical.as_deref(),
            Self::Invalid { lexical, .. } => Some(lexical),
            Self::Null { .. } => None,
        }
    }
    /// CSV preserves scalar XML spelling. Binary/complex output is explicitly lossy;
    /// tagged JSON is the full typed export. Nulls are unquoted empty CSV fields.
    pub fn csv_text(&self) -> Option<String> {
        match self {
            Self::Null { .. } => None,
            Self::Value {
                value: Value::Binary(bytes),
                ..
            } => Some(format!("<lossy: binary {} bytes>", bytes.len())),
            Self::Value {
                value: Value::Complex(records),
                ..
            } => Some(format!("<lossy: complex {} records>", records.len())),
            Self::Value {
                lexical: Some(s), ..
            }
            | Self::Invalid { lexical: s, .. } => Some(s.clone()),
            Self::Value {
                value,
                lexical: None,
            } => Some(format!("<lossy: {value:?}>")),
        }
    }
}

/// Exact signed decimal, kept as validated decimal text without floating conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct Decimal(String);
impl Decimal {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim_matches(xml_space);
        let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
        if digits.is_empty()
            || !digits.chars().any(|c| c.is_ascii_digit())
            || digits.chars().any(|c| !c.is_ascii_digit() && c != '.')
            || digits.matches('.').count() > 1
        {
            return Err("invalid XSD decimal".into());
        }
        Ok(Self(s.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
/// Access currency as an exact signed integer scaled by 10,000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Currency(i64);
impl Currency {
    pub fn scaled(self) -> i64 {
        self.0
    }
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim_matches(xml_space);
        let (mantissa, exponent) = match s.split_once(['e', 'E']) {
            Some((m, e)) => (
                m,
                e.parse::<i32>().map_err(|_| "invalid currency exponent")?,
            ),
            None => (s, 0),
        };
        let dec = Decimal::parse(mantissa)?;
        let negative = dec.as_str().starts_with('-');
        let unsigned = dec.as_str().trim_start_matches(['+', '-']);
        let scale = unsigned.split_once('.').map(|(_, f)| f.len()).unwrap_or(0);
        let mut digits = unsigned
            .replace('.', "")
            .trim_start_matches('0')
            .to_string();
        if digits.is_empty() {
            return Ok(Self(0));
        }
        let shift = i64::from(exponent) + 4
            - i64::try_from(scale).map_err(|_| "currency scale too large")?;
        if shift < 0 {
            let remove = usize::try_from(-shift)
                .map_err(|_| "currency precision exceeds four decimal places")?;
            if remove > digits.len() || !digits[digits.len() - remove..].chars().all(|c| c == '0') {
                return Err("currency not exactly representable at four decimal places".into());
            }
            digits.truncate(digits.len() - remove);
        } else {
            if shift > 19 || digits.len() + shift as usize > 19 {
                return Err("currency overflow".into());
            }
            digits.extend(std::iter::repeat_n('0', shift as usize));
        }
        if negative {
            digits.insert(0, '-');
        }
        digits
            .parse::<i64>()
            .map(Self)
            .map_err(|_| "currency overflow".into())
    }
}
impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let n = self.0.unsigned_abs();
        write!(
            f,
            "{}{}.{:04}",
            if self.0 < 0 { "-" } else { "" },
            n / 10000,
            n % 10000
        )
    }
}
#[cfg(feature = "serde")]
impl serde::Serialize for Currency {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}
/// XML Schema 1.0 calendar/time fields; no host timezone conversion.
/// Years outside the signed 32-bit range are explicitly unsupported. 24:00:00 is
/// retained as hour 24 (only allowed with zero minutes, seconds and fraction).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct XmlDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub fractional_digits: String,
    pub offset_minutes: Option<i16>,
}
fn xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}
impl XmlDateTime {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim_matches(xml_space);
        let (date, time) = s.split_once('T').ok_or("expected dateTime T separator")?;
        let mut parts = date.rsplitn(3, '-');
        let day_s = parts.next().ok_or("missing day")?;
        let month_s = parts.next().ok_or("missing month")?;
        let year_s = parts.next().ok_or("missing year")?;
        let year_digits = year_s.strip_prefix('-').unwrap_or(year_s);
        if year_digits.len() < 4
            || !year_digits.bytes().all(|c| c.is_ascii_digit())
            || (year_digits.len() > 4 && year_digits.starts_with('0'))
        {
            return Err("invalid dateTime year".into());
        }
        let year = year_s
            .parse::<i32>()
            .map_err(|_| "unsupported dateTime year")?;
        if year == 0 {
            return Err("year zero is not valid in XML Schema 1.0".into());
        }
        let two = |s: &str| -> Result<u8, String> {
            if s.len() != 2 || !s.bytes().all(|c| c.is_ascii_digit()) {
                return Err("expected two decimal digits".into());
            }
            s.parse().map_err(|_| "invalid digits".into())
        };
        let month = two(month_s)?;
        let day = two(day_s)?;
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return Err("invalid month".into()),
        };
        if day == 0 || day > days {
            return Err("invalid calendar day".into());
        }
        let (clock, offset_minutes) = if let Some(t) = time.strip_suffix('Z') {
            (t, Some(0))
        } else if let Some(i) = time.find(['+', '-']) {
            let zone = &time[i + 1..];
            let (h, m) = zone.split_once(':').ok_or("invalid timezone")?;
            let h = two(h)?;
            let m = two(m)?;
            if h > 14 || m > 59 || (h == 14 && m != 0) {
                return Err("timezone outside ±14:00".into());
            }
            (
                &time[..i],
                Some(
                    (i16::from(h) * 60 + i16::from(m))
                        * if time.as_bytes()[i] == b'-' { -1 } else { 1 },
                ),
            )
        } else {
            (time, None)
        };
        let components: Vec<_> = clock.split(':').collect();
        if components.len() != 3 {
            return Err("expected hh:mm:ss".into());
        }
        let hour = two(components[0])?;
        let minute = two(components[1])?;
        let (seconds, fraction) = match components[2].split_once('.') {
            Some((s, f)) if !f.is_empty() && f.bytes().all(|c| c.is_ascii_digit()) => (s, f),
            Some(_) => return Err("invalid fractional seconds".into()),
            None => (components[2], ""),
        };
        let second = two(seconds)?;
        if hour > 24
            || minute > 59
            || second > 59
            || (hour == 24 && (minute != 0 || second != 0 || fraction.bytes().any(|b| b != b'0')))
        {
            return Err("invalid clock time".into());
        }
        Ok(Self {
            year,
            month,
            day,
            hour,
            minute,
            second,
            fractional_digits: fraction.into(),
            offset_minutes,
        })
    }
}

impl Column {
    /// Strict decoding for known Jet/XSD types. The package reader retains failures in Cell::Invalid.
    pub fn decode(&self, lexical: &str) -> Result<Value, ValueError> {
        self.decode_inner(lexical)
            .map_err(|reason| ValueError::for_column(&self.name, reason))
    }
    fn decode_inner(&self, lexical: &str) -> Result<Value, String> {
        let s = lexical.trim_matches(xml_space);
        let xsd = self
            .xsd_type
            .as_ref()
            .filter(|n| n.namespace.as_deref() == Some(XSD))
            .map(|n| n.local.as_str());
        // The XSD lexical contract is checked even when Jet supplies the public type.
        if let Some(kind) = xsd {
            validate_xsd(kind, s)?;
        }
        let inferred;
        let jet = if matches!(self.jet_type, JetType::Other(_)) {
            inferred = match xsd {
                Some("string" | "normalizedString" | "token") => JetType::Text,
                Some("boolean") => JetType::YesNo,
                Some(
                    "byte" | "short" | "int" | "long" | "integer" | "unsignedByte"
                    | "unsignedShort" | "unsignedInt",
                ) => JetType::LongInteger,
                Some("float") => JetType::Single,
                Some("double") => JetType::Double,
                Some("decimal") => JetType::Decimal,
                Some("dateTime") => JetType::DateTime,
                Some("base64Binary") => JetType::OleObject,
                _ => {
                    return Ok(Value::Uninterpreted {
                        xsd_type: self.xsd_type.clone(),
                        text: lexical.into(),
                    });
                }
            };
            &inferred
        } else {
            &self.jet_type
        };
        Ok(match jet {
            JetType::Text | JetType::Memo | JetType::Hyperlink => Value::Text(lexical.into()),
            JetType::YesNo => Value::Boolean(match s {
                "1" | "true" => true,
                "0" | "false" => false,
                _ => return Err("invalid XML boolean (expected true/false/1/0)".into()),
            }),
            JetType::Byte | JetType::Integer | JetType::LongInteger | JetType::AutoNumber => {
                let n = s
                    .parse::<i64>()
                    .map_err(|_| "invalid or overflowing integer")?;
                let valid = match jet {
                    JetType::Byte => (0..=255).contains(&n),
                    JetType::Integer => i16::try_from(n).is_ok(),
                    _ if !matches!(self.jet_type, JetType::Other(_)) => i32::try_from(n).is_ok(),
                    _ => true,
                };
                if !valid {
                    return Err("integer outside Jet field range".into());
                }
                Value::Integer(n)
            }
            JetType::Currency => Value::Currency(Currency::parse(s)?),
            JetType::Decimal => Value::Decimal(Decimal::parse(s)?),
            JetType::DateTime => Value::DateTime(XmlDateTime::parse(s)?),
            JetType::Single => {
                let d = parse_double(s)?;
                let f = d as f32;
                if d.is_finite() && !f.is_finite() {
                    return Err("single overflow".into());
                }
                Value::Single(f)
            }
            JetType::Double => Value::Double(parse_double(s)?),
            JetType::OleObject => {
                use base64::Engine;
                let compact: String = lexical.chars().filter(|c| !xml_space(*c)).collect();
                Value::Binary(
                    base64::engine::general_purpose::STANDARD
                        .decode(compact)
                        .map_err(|e| format!("invalid base64: {e}"))?,
                )
            }
            JetType::Guid => {
                let s = s
                    .strip_prefix('{')
                    .and_then(|v| v.strip_suffix('}'))
                    .unwrap_or(s);
                if s.len() != 36 || ![8, 13, 18, 23].iter().all(|&i| s.as_bytes()[i] == b'-') {
                    return Err("invalid GUID shape".into());
                }
                let hex: String = s.chars().filter(|c| *c != '-').collect();
                if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err("invalid GUID digits".into());
                }
                let mut bytes = [0; 16];
                for (i, byte) in bytes.iter_mut().enumerate() {
                    *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16)
                        .map_err(|_| "invalid GUID")?;
                }
                Value::Guid(bytes)
            }
            JetType::Complex => return Err("complex field requires child records".into()),
            JetType::Other(_) => unreachable!(),
        })
    }
}
fn parse_double(s: &str) -> Result<f64, String> {
    match s {
        "INF" => return Ok(f64::INFINITY),
        "-INF" => return Ok(f64::NEG_INFINITY),
        "NaN" => return Ok(f64::NAN),
        _ => (),
    }
    let (m, exp) = s
        .split_once(['e', 'E'])
        .map(|(m, e)| (m, Some(e)))
        .unwrap_or((s, None));
    Decimal::parse(m)?;
    if let Some(e) = exp {
        let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
        if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
            return Err("invalid floating exponent".into());
        }
    }
    let n = s.parse::<f64>().map_err(|_| "invalid floating value")?;
    if !n.is_finite() {
        return Err("floating overflow; use explicit INF/-INF".into());
    }
    Ok(n)
}
fn validate_xsd(kind: &str, s: &str) -> Result<(), String> {
    match kind {
        "boolean" if !matches!(s, "true" | "false" | "1" | "0") => {
            return Err("invalid XSD boolean".into());
        }
        "decimal" => {
            Decimal::parse(s)?;
        }
        "dateTime" => {
            XmlDateTime::parse(s)?;
        }
        "float" | "double" => {
            parse_double(s)?;
        }
        "byte" | "short" | "int" | "long" | "integer" | "unsignedByte" | "unsignedShort"
        | "unsignedInt" => {
            let n = s
                .parse::<i64>()
                .map_err(|_| "invalid or unsupported XSD integer")?;
            let valid = match kind {
                "byte" => i8::try_from(n).is_ok(),
                "short" => i16::try_from(n).is_ok(),
                "int" => i32::try_from(n).is_ok(),
                "unsignedByte" => u8::try_from(n).is_ok(),
                "unsignedShort" => u16::try_from(n).is_ok(),
                "unsignedInt" => u32::try_from(n).is_ok(),
                _ => true,
            };
            if !valid {
                return Err("value outside XSD integer range".into());
            }
        }
        "base64Binary" => {
            use base64::Engine;
            let compact: String = s.chars().filter(|c| !xml_space(*c)).collect();
            base64::engine::general_purpose::STANDARD
                .decode(compact)
                .map_err(|_| "invalid XSD base64")?;
        }
        _ => (),
    }
    Ok(())
}

/// A table that is a link to a SharePoint list rather than local data.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SharePointList {
    /// List template id (100 generic, 105 contacts, 106 events, 107 tasks, 1100 issues, …).
    pub template_id: Option<u32>,
    pub root_folder: Option<String>,
    pub default_view_url: Option<String>,
    pub version: Option<String>,
    pub last_modified: Option<String>,
    pub display_views_on_site: bool,
    pub document_library: bool,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    /// `od:tableProperty` entries: `(name, type code, value)`.
    pub properties: Vec<(String, String, String)>,
    /// Present when the table is a SharePoint list link (`WSS*` table properties).
    pub sharepoint: Option<SharePointList>,
    /// Rows in column order, preserving absent, nil, invalid and typed values.
    pub rows: Vec<Row>,
    /// Whether a sample-data part existed (a table can legitimately have zero rows).
    pub has_data_part: bool,
}

impl Table {
    pub fn primary_key(&self) -> Option<&Index> {
        self.indexes.iter().find(|i| i.primary)
    }

    pub fn property(&self, name: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, _, v)| v.as_str())
    }

    /// True for a SharePoint list link whose rows live on the server. A table that carries
    /// `WSS*` properties *and* a data part is local data that Access can publish to a list
    /// of that template id when the database is created on a site (the 2007 desktop
    /// templates ship this way); it is not linked.
    pub fn is_linked(&self) -> bool {
        self.sharepoint.is_some() && !self.has_data_part
    }

    /// Column by name, case-insensitively (Access names are case-insensitive).
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }

    /// RFC 4180 CSV with a header row; every value quoted.
    pub fn to_csv(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = self
            .columns
            .iter()
            .map(|c| q(&c.name))
            .collect::<Vec<_>>()
            .join(",");
        out.push('\n');
        for row in &self.rows {
            let line: Vec<String> = row
                .cells
                .iter()
                .map(|v| v.csv_text().map(|s| q(&s)).unwrap_or_default())
                .collect();
            out.push_str(&line.join(","));
            out.push('\n');
        }
        out
    }
}

pub(crate) fn parse_schema(part: &str, name: &str, bytes: &[u8]) -> crate::Result<Table> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    let table_el = doc
        .descendants()
        .filter(|n| n.has_tag_name((XSD, "element")))
        .find(|n| {
            n.attribute("name").is_some_and(|a| a != "dataroot")
                && n.parent().is_some_and(|p| p.has_tag_name((XSD, "schema")))
        })
        .ok_or_else(|| crate::Error::Invalid {
            part: part.to_string(),
            reason: "no table element in schema".into(),
        })?;
    let mut table = Table {
        name: name.to_string(),
        columns: Vec::new(),
        indexes: Vec::new(),
        properties: Vec::new(),
        sharepoint: None,
        rows: Vec::new(),
        has_data_part: false,
    };
    for n in table_el.descendants() {
        if n.has_tag_name((OD, "index")) {
            let key = n.attribute("index-key").unwrap_or("");
            table.indexes.push(Index {
                name: n.attribute("index-name").unwrap_or("").to_string(),
                columns: key.split_whitespace().map(unescape_xml_name).collect(),
                primary: n.attribute("primary") == Some("yes"),
                unique: n.attribute("unique") == Some("yes"),
                order: n
                    .attribute("order")
                    .unwrap_or("")
                    .split_whitespace()
                    .map(String::from)
                    .collect(),
            });
        } else if n.has_tag_name((OD, "tableProperty")) {
            table.properties.push((
                n.attribute("name").unwrap_or("").to_string(),
                n.attribute("type").unwrap_or("").to_string(),
                n.attribute("value").unwrap_or("").trim().to_string(),
            ));
        }
    }
    if table
        .properties
        .iter()
        .any(|(n, _, _)| n.starts_with("WSS"))
    {
        table.sharepoint = Some(SharePointList {
            template_id: table.property("WSSTemplateID").and_then(|v| v.parse().ok()),
            root_folder: table.property("WSSRootFolder").map(String::from),
            default_view_url: table.property("DefaultViewUrl").map(String::from),
            version: table.property("WSSVersion").map(String::from),
            last_modified: table.property("WSSLastModified").map(String::from),
            display_views_on_site: table.property("DisplayViewsOnSharePointSite") == Some("1"),
            document_library: table.property("DocumentLibrary") == Some("1"),
        });
    }
    let sequence = table_el
        .descendants()
        .find(|n| n.has_tag_name((XSD, "sequence")));
    if let Some(seq) = sequence {
        for col in seq.children().filter(|n| n.has_tag_name((XSD, "element"))) {
            table.columns.push(parse_column(col));
        }
    }
    Ok(table)
}

fn expanded_name(node: roxmltree::Node<'_, '_>, raw: &str) -> ExpandedName {
    let (prefix, local) = raw
        .split_once(':')
        .map(|(p, l)| (Some(p), l))
        .unwrap_or((None, raw));
    ExpandedName {
        namespace: node.lookup_namespace_uri(prefix).map(String::from),
        local: local.into(),
    }
}
fn parse_column(col: roxmltree::Node<'_, '_>) -> Column {
    // Stop metadata traversal at nested field declarations so child restrictions and
    // properties never leak into their parent attachment column.
    let local: Vec<_> = col
        .descendants()
        .filter(|d| {
            d.ancestors()
                .skip(1)
                .take_while(|a| *a != col)
                .all(|a| !a.has_tag_name((XSD, "element")))
        })
        .collect();
    let restriction = local.iter().find(|n| n.has_tag_name((XSD, "restriction")));
    let xsd_type = col
        .attribute("type")
        .map(|s| expanded_name(col, s))
        .or_else(|| restriction.and_then(|n| n.attribute("base").map(|s| expanded_name(*n, s))));
    let restrictions: Vec<_> = restriction
        .into_iter()
        .flat_map(|n| n.children())
        .filter(|n| n.is_element())
        .filter_map(|n| {
            n.attribute("value").map(|v| Restriction {
                name: n.tag_name().name().into(),
                value: v.into(),
            })
        })
        .collect();
    let children: Vec<Column> = local
        .iter()
        .find(|n| n.has_tag_name((XSD, "sequence")))
        .map(|seq| {
            seq.children()
                .filter(|n| n.has_tag_name((XSD, "element")))
                .map(parse_column)
                .collect()
        })
        .unwrap_or_default();
    Column {
        name: unescape_xml_name(col.attribute("name").unwrap_or("")),
        jet_type: col
            .attribute((OD, "jetType"))
            .map(JetType::parse)
            .unwrap_or(JetType::Other(String::new())),
        sql_type: col.attribute((OD, "sqlSType")).map(String::from),
        required: col.attribute((OD, "nonNullable")) == Some("yes"),
        auto_increment: col.attribute((OD, "autoUnique")) == Some("yes"),
        max_length: restrictions
            .iter()
            .find(|r| r.name == "maxLength")
            .and_then(|r| r.value.parse().ok()),
        complex_type: col.attribute((OD, "jetComplexType")).map(String::from),
        properties: local
            .iter()
            .filter(|n| n.has_tag_name((OD, "fieldProperty")))
            .map(|n| {
                (
                    n.attribute("name").unwrap_or("").into(),
                    n.attribute("type").unwrap_or("").into(),
                    n.attribute("value").unwrap_or("").into(),
                )
            })
            .collect(),
        xsd_type,
        restrictions,
        children,
    }
}

/// Read rows in schema order while retaining absent versus xsi:nil and malformed cells.
pub(crate) fn parse_data(part: &str, table: &mut Table, bytes: &[u8]) -> crate::Result<()> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    table.has_data_part = true;
    let Some(dataroot) = doc
        .descendants()
        .find(|n| n.is_element() && n.tag_name().name() == "dataroot")
    else {
        return Ok(());
    };
    for row in dataroot.children().filter(|n| n.is_element()) {
        let mut cells = vec![
            Cell::Null {
                encoding: NullEncoding::Absent
            };
            table.columns.len()
        ];
        for cell in row.children().filter(|n| n.is_element()) {
            let name = unescape_xml_name(cell.tag_name().name());
            let index = match table.columns.iter().position(|c| c.name == name) {
                Some(i) => i,
                None => {
                    table.columns.push(Column::unknown(&name));
                    for r in &mut table.rows {
                        r.cells.push(Cell::Null {
                            encoding: NullEncoding::Absent,
                        });
                    }
                    cells.push(Cell::Null {
                        encoding: NullEncoding::Absent,
                    });
                    table.columns.len() - 1
                }
            };
            let decoded = decode_element(
                cell,
                &table.columns[index],
                part,
                &table.name,
                table.rows.len(),
                &name,
            );
            match (&mut cells[index], decoded) {
                (
                    Cell::Value {
                        value: Value::Complex(existing),
                        ..
                    },
                    Cell::Value {
                        value: Value::Complex(mut records),
                        ..
                    },
                ) => existing.append(&mut records),
                (slot, value) => *slot = value,
            }
        }
        table.rows.push(Row { cells });
    }
    Ok(())
}
impl Column {
    fn unknown(name: &str) -> Self {
        Self {
            name: name.into(),
            jet_type: JetType::Other(String::new()),
            sql_type: None,
            required: false,
            auto_increment: false,
            max_length: None,
            complex_type: None,
            properties: Vec::new(),
            xsd_type: None,
            restrictions: Vec::new(),
            children: Vec::new(),
        }
    }
}
fn decode_element(
    node: roxmltree::Node<'_, '_>,
    column: &Column,
    part: &str,
    table: &str,
    row: usize,
    path: &str,
) -> Cell {
    const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
    let invalid = |lexical: String, reason: String| Cell::Invalid {
        lexical,
        error: ValueError {
            part: part.into(),
            table: table.into(),
            row: Some(row),
            column: path.into(),
            reason,
        },
    };
    let lexical: String = node
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    if let Some(nil) = node.attribute((XSI, "nil")) {
        match nil.trim_matches(xml_space) {
            "true" | "1" if !node.children().any(|n| n.is_element()) && lexical.is_empty() => {
                return Cell::Null {
                    encoding: NullEncoding::XsiNil,
                };
            }
            "true" | "1" => return invalid(lexical, "xsi:nil cell contains content".into()),
            "false" | "0" => (),
            _ => return invalid(lexical, "invalid xsi:nil boolean".into()),
        }
    }
    if node.children().any(|n| n.is_element()) || column.jet_type == JetType::Complex {
        let mut fields = Vec::new();
        for child in node.children().filter(|n| n.is_element()) {
            let name = unescape_xml_name(child.tag_name().name());
            let fallback = Column::unknown(&name);
            let schema = column
                .children
                .iter()
                .find(|c| c.name == name)
                .unwrap_or(&fallback);
            let cell = decode_element(child, schema, part, table, row, &format!("{path}.{name}"));
            fields.push((name, cell));
        }
        // Retain lexical order of present/repeated fields; absent children follow in schema order.
        for schema in &column.children {
            if !fields.iter().any(|(name, _)| name == &schema.name) {
                fields.push((
                    schema.name.clone(),
                    Cell::Null {
                        encoding: NullEncoding::Absent,
                    },
                ));
            }
        }
        return Cell::Value {
            value: Value::Complex(vec![ComplexRecord { fields }]),
            lexical: None,
        };
    }
    match column.decode(&lexical) {
        Ok(value) => Cell::Value {
            value,
            lexical: Some(lexical),
        },
        Err(e) => invalid(lexical, e.reason),
    }
}
impl Table {
    /// Reject every malformed scalar, including attachment children, with data-part context.
    pub fn validate_values(&self) -> Result<(), Vec<ValueError>> {
        fn errors(cell: &Cell, out: &mut Vec<ValueError>) {
            match cell {
                Cell::Invalid { error, .. } => out.push(error.clone()),
                Cell::Value {
                    value: Value::Complex(records),
                    ..
                } => {
                    for r in records {
                        for (_, c) in &r.fields {
                            errors(c, out);
                        }
                    }
                }
                _ => (),
            }
        }
        let mut out = Vec::new();
        for r in &self.rows {
            for c in &r.cells {
                errors(c, &mut out);
            }
        }
        if out.is_empty() { Ok(()) } else { Err(out) }
    }
}
