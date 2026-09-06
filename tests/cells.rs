use std::io::{Cursor, Write};

use accdt::*;
use zip::write::SimpleFileOptions;

fn table(fields: &str, rows: &str) -> Table {
    let schema = format!(
        r#"<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:od="urn:schemas-microsoft-com:officedata" xmlns:custom="urn:custom"><xs:element name="T"><xs:complexType><xs:sequence>{fields}</xs:sequence></xs:complexType></xs:element></xs:schema>"#
    );
    let data = format!(
        r#"<dataroot xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">{rows}</dataroot>"#
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", "<Types/>"),
        ("template/database/objects/tableT.xsd", &schema),
        ("template/database/objects/sampleData/tableT.xml", &data),
        (
            "template/database/objects/_rels/tableT.xsd.rels",
            r#"<Relationships><Relationship Id="data" Type="http://schemas.microsoft.com/office/access/2005/04/template/table-data" Target="sampleData/tableT.xml"/></Relationships>"#,
        ),
    ] {
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(bytes.as_bytes()).unwrap();
    }
    Package::from_bytes(&zip.finish().unwrap().into_inner())
        .unwrap()
        .table("T")
        .unwrap()
}
fn column(jet: &str, xsd: &str) -> Column {
    table(
        &format!(r#"<xs:element name="C" od:jetType="{jet}" type="xs:{xsd}"/>"#),
        "",
    )
    .columns
    .remove(0)
}

#[test]
fn boolean_integer_and_invalid_context() {
    let c = column("yesno", "boolean");
    for (s, b) in [("0", false), ("1", true), ("true", true), ("false", false)] {
        assert_eq!(c.decode(s).unwrap(), Value::Boolean(b));
    }
    for s in ["-1", "True", "NotDefault", "", "yes"] {
        assert!(c.decode(s).is_err(), "{s}");
    }
    for (jet, xsd, good, bad) in [
        ("byte", "unsignedByte", "255", "256"),
        ("integer", "short", "-32768", "32768"),
        ("longinteger", "int", "2147483647", "2147483648"),
    ] {
        let c = column(jet, xsd);
        assert!(c.decode(good).is_ok());
        assert!(c.decode(bad).is_err());
    }
    let t = table(
        r#"<xs:element name="C" od:jetType="yesno" type="xs:boolean"/>"#,
        "<T><C>1</C></T><T><C>bad</C></T><T><C>0</C></T>",
    );
    assert_eq!(t.rows.len(), 3);
    let errors = t.validate_values().unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].row, Some(1));
    assert_eq!(errors[0].column, "C");
    assert_eq!(errors[0].table, "T");
    assert!(errors[0].part.ends_with("tableT.xml"));
    assert_eq!(t.rows[1].cells[0].lexical(), Some("bad"));
}

#[test]
fn exact_currency_decimal_and_floats() {
    let c = column("currency", "double");
    for (s, scaled) in [
        ("12.3400", 123400),
        ("1234e-2", 123400),
        ("-0.0001", -1),
        ("922337203685477.5807", i64::MAX),
        ("-922337203685477.5808", i64::MIN),
        ("0e99999", 0),
    ] {
        let Value::Currency(v) = c.decode(s).unwrap() else {
            panic!()
        };
        assert_eq!(v.scaled(), scaled, "{s}");
    }
    for s in [
        "0.00001",
        "922337203685477.5808",
        "-922337203685477.5809",
        "1e100000",
        "NaN",
        "INF",
    ] {
        assert!(c.decode(s).is_err(), "{s}");
    }
    let huge = "12345678901234567890123456789012345678901234567890.0000000000001";
    let Value::Decimal(d) = column("decimal", "decimal").decode(huge).unwrap() else {
        panic!()
    };
    assert_eq!(d.as_str(), huge);
    assert!(column("decimal", "decimal").decode("1e2").is_err());
    assert_eq!(
        column("double", "double").decode("1.25e2").unwrap(),
        Value::Double(125.0)
    );
    assert_eq!(
        column("single", "float").decode("1.25").unwrap(),
        Value::Single(1.25)
    );
    assert!(column("single", "float").decode("1e39").is_err());
    assert!(column("double", "double").decode("infinity").is_err());
    assert!(
        matches!(column("double", "double").decode("NaN").unwrap(), Value::Double(n) if n.is_nan())
    );
}

#[test]
fn datetime_keeps_calendar_fraction_and_optional_zone() {
    let c = column("datetime", "dateTime");
    for s in [
        "2024-02-29T13:45:59.12345678901234567890-07:30",
        "2000-01-01T00:00:00",
        "2020-12-31T24:00:00Z",
        "-0001-01-01T00:00:00+14:00",
    ] {
        assert!(c.decode(s).is_ok(), "{s}");
    }
    let Value::DateTime(d) = c
        .decode("2024-02-29T13:45:59.12345678901234567890-07:30")
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(d.offset_minutes, Some(-450));
    assert_eq!(d.fractional_digits, "12345678901234567890");
    assert_eq!((d.year, d.month, d.day, d.hour), (2024, 2, 29, 13));
    let Value::DateTime(d) = c.decode("2000-01-01T00:00:00").unwrap() else {
        panic!()
    };
    assert_eq!(d.offset_minutes, None);
    for s in [
        "2023-02-29T00:00:00",
        "1900-02-29T00:00:00",
        "2000-01-01",
        "0000-01-01T00:00:00",
        "2024-01-01T24:00:00.1",
        "2024-01-01T00:00:00+14:01",
        "2024-01-01T23:59:60",
        "2024-01-01T00:00:00.",
        "2024-13-01T00:00:00",
    ] {
        assert!(c.decode(s).is_err(), "{s}");
    }
}

#[test]
fn null_empty_unknown_binary_and_csv_contract() {
    let fields = r#"<xs:element name="Text" od:jetType="text" type="xs:string"/><xs:element name="Absent" od:jetType="text" type="xs:string"/><xs:element name="Nil" od:jetType="text" type="xs:string"/><xs:element name="Unknown" type="custom:Future"/><xs:element name="B" od:jetType="oleobject" type="xs:base64Binary"/><xs:element name="D" od:jetType="double" type="xs:double"/>"#;
    let t = table(
        fields,
        r#"<T><Text/><Nil xsi:nil="true"/><Unknown>  future  </Unknown><B> Y W J j </B><D>1.00e+2</D></T>"#,
    );
    let row = &t.rows[0].cells;
    assert_eq!(row[0].value(), Some(&Value::Text("".into())));
    assert!(matches!(
        row[1],
        Cell::Null {
            encoding: NullEncoding::Absent
        }
    ));
    assert!(matches!(
        row[2],
        Cell::Null {
            encoding: NullEncoding::XsiNil
        }
    ));
    assert!(
        matches!(row[3].value(), Some(Value::Uninterpreted { xsd_type: Some(n), text }) if n.namespace.as_deref() == Some("urn:custom") && text == "  future  ")
    );
    assert_eq!(row[4].value(), Some(&Value::Binary(b"abc".to_vec())));
    assert_eq!(
        t.to_csv(),
        "\"Text\",\"Absent\",\"Nil\",\"Unknown\",\"B\",\"D\"\n\"\",,,\"  future  \",\"<lossy: binary 3 bytes>\",\"1.00e+2\"\n"
    );
    for s in ["%%%", "A===", "YQ", "YR=="] {
        assert!(
            column("oleobject", "base64Binary").decode(s).is_err(),
            "{s}"
        );
    }
    assert_eq!(
        column("oleobject", "base64Binary").decode("").unwrap(),
        Value::Binary(Vec::new())
    );
    let t = table(fields, r#"<T><Nil xsi:nil="true">not nil</Nil></T>"#);
    assert!(t.validate_values().is_err());
}

#[test]
fn attachment_child_schema_order_repetition_and_whitespace() {
    let fields = r#"<xs:element name="Files" od:jetType="complex" od:jetComplexType="MSysComplexType_Attachment"><xs:complexType><xs:sequence><xs:element name="FileName" od:jetType="text"><xs:simpleType><xs:restriction base="xs:string"><xs:maxLength value="255"/></xs:restriction></xs:simpleType></xs:element><xs:element name="FileData" od:jetType="oleobject" type="xs:base64Binary"/><xs:element name="FileTimeStamp" od:jetType="datetime" type="xs:dateTime"/><xs:element name="FileFlags" od:jetType="longinteger" type="xs:int"/></xs:sequence></xs:complexType></xs:element>"#;
    let t = table(
        fields,
        "<T><Files><FileName>  a file.png  </FileName><FileData>YWJj</FileData><FileTimeStamp>2024-01-02T03:04:05Z</FileTimeStamp><FileFlags>1</FileFlags><Extra>x</Extra><Extra>y</Extra></Files><Files><FileName>b.png</FileName><FileData>bad!</FileData></Files></T>",
    );
    assert_eq!(t.columns[0].max_length, None);
    assert_eq!(t.columns[0].children.len(), 4);
    assert_eq!(t.columns[0].children[0].max_length, Some(255));
    assert_eq!(
        t.columns[0].children[0].xsd_type.as_ref().unwrap().local,
        "string"
    );
    let Some(Value::Complex(records)) = t.rows[0].cells[0].value() else {
        panic!()
    };
    assert_eq!(records.len(), 2);
    let fields = &records[0].fields;
    assert_eq!(
        fields[0].1.value(),
        Some(&Value::Text("  a file.png  ".into()))
    );
    assert_eq!(fields[1].1.value(), Some(&Value::Binary(b"abc".to_vec())));
    assert!(matches!(fields[2].1.value(), Some(Value::DateTime(_))));
    assert_eq!(fields[3].1.value(), Some(&Value::Integer(1)));
    assert_eq!(fields[4].0, "Extra");
    assert_eq!(fields[5].0, "Extra");
    let errors = t.validate_values().unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].column, "Files.FileData");
    assert!(matches!(
        records[1].fields[2].1,
        Cell::Null {
            encoding: NullEncoding::Absent
        }
    ));
}

#[cfg(feature = "serde")]
#[test]
fn json_is_tagged_exact_and_nonfinite_safe() {
    let t = table(
        r#"<xs:element name="Money" od:jetType="currency" type="xs:double"/><xs:element name="N" od:jetType="double" type="xs:double"/><xs:element name="Missing" od:jetType="text" type="xs:string"/>"#,
        "<T><Money>1.25e2</Money><N>INF</N></T>",
    );
    let json = serde_json::to_value(&t.rows).unwrap();
    assert_eq!(json[0]["cells"][0]["kind"], "Value");
    assert_eq!(json[0]["cells"][0]["value"]["type"], "Currency");
    assert_eq!(json[0]["cells"][0]["value"]["value"], "125.0000");
    assert_eq!(json[0]["cells"][0]["lexical"], "1.25e2");
    assert_eq!(json[0]["cells"][1]["value"]["value"], "INF");
    assert_eq!(json[0]["cells"][2]["encoding"], "Absent");
    assert_eq!(
        serde_json::to_value(Value::Double(f64::NAN)).unwrap()["value"],
        "NaN"
    );
    assert_eq!(
        serde_json::to_value(Value::Single(f32::NEG_INFINITY)).unwrap()["value"],
        "-INF"
    );
    assert_eq!(
        serde_json::to_value(Value::Double(1.5)).unwrap()["value"],
        1.5
    );
}
