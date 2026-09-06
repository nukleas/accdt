//! A package built in memory, covering every part type.

use std::io::{Cursor, Write};

use accdt::{JetType, ObjectKind, Operation, Package, SqlSource, Value};
use zip::write::SimpleFileOptions;

fn utf16(s: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

fn build() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut add = |name: &str, bytes: &[u8]| {
        zip.start_file(name, opts).unwrap();
        zip.write_all(bytes).unwrap();
    };
    add(
        "[Content_Types].xml",
        b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>",
    );
    add("docProps/core.xml", b"<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Demo</dc:title><dc:description>A test</dc:description></cp:coreProperties>");
    add("template/template.xml", b"<Template xmlns=\"http://schemas.microsoft.com/office/access/2005/04/template/start\"><TemplateFormat>2</TemplateFormat><RequiredAccessVersion>14</RequiredAccessVersion><Type>Getting Started</Type></Template>");
    add(
        "template/database/databaseProperties.xml",
        &utf16(
            "<?xml version=\"1.0\" encoding=\"UTF-16\" standalone=\"no\"?><Application xmlns=\"x\"><Properties><Property Name=\"AccessVersion\" Type=\"10\">09.50</Property><Property Name=\"StartUpForm\" Type=\"10\">frmMain</Property></Properties></Application>",
        ),
    );
    add("template/database/relationships.xml", b"<dataroot><MSysRelationships><ccolumn>1</ccolumn><grbit>4352</grbit><icolumn>0</icolumn><szColumn>CompanyID</szColumn><szObject>Orders</szObject><szReferencedColumn>ID</szReferencedColumn><szReferencedObject>Companies</szReferencedObject><szRelationship>CompaniesOrders</szRelationship></MSysRelationships></dataroot>");
    add("template/database/vbaReferences.xml", b"<VBAReferences xmlns=\"v\"><VBAReference><GUID>{000204EF-0000-0000-C000-000000000046}</GUID><MajorVer>4</MajorVer><MinorVer>2</MinorVer></VBAReference><VBAReference><GUID>{831FDD16-0C5C-11D2-A9FC-0000F8754DA1}</GUID><MajorVer>2</MajorVer><MinorVer>0</MinorVer></VBAReference></VBAReferences>");
    // Table with data, metadata, rels, data macro.
    add("template/database/objects/tableCompanies.xsd", br#"<?xml version="1.0"?><xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" xmlns:od="urn:schemas-microsoft-com:officedata"><xsd:element name="dataroot"/><xsd:element name="Companies"><xsd:annotation><xsd:appinfo><od:index index-name="PrimaryKey" index-key="ID " primary="yes" unique="yes" clustered="no" order="asc"/><od:tableProperty name="DefaultView" type="2" value="2"/></xsd:appinfo></xsd:annotation><xsd:complexType><xsd:sequence><xsd:element name="ID" minOccurs="1" od:jetType="autonumber" od:sqlSType="int" od:autoUnique="yes" od:nonNullable="yes" type="xsd:int"/><xsd:element name="Company_x0020_Name" minOccurs="0" od:jetType="text" od:sqlSType="nvarchar"><xsd:annotation><xsd:appinfo><od:fieldProperty name="Description" type="10" value="Legal name"/></xsd:appinfo></xsd:annotation><xsd:simpleType><xsd:restriction base="xsd:string"><xsd:maxLength value="50"/></xsd:restriction></xsd:simpleType></xsd:element><xsd:element name="Active" minOccurs="0" od:jetType="yesno" od:sqlSType="bit" type="xsd:boolean"/></xsd:sequence></xsd:complexType></xsd:element></xsd:schema>"#);
    add("template/database/objects/sampleData/tableCompanies.xml", b"<root><dataroot><Companies><ID>1</ID><Company_x0020_Name>Acme &amp; Co</Company_x0020_Name><Active>1</Active><Files><FileName>a.png</FileName><FileData>AAAA</FileData></Files><Files><FileName>b.png</FileName><FileData>BBBB</FileData></Files></Companies><Companies><ID>2</ID><Active>0</Active></Companies></dataroot></root>");
    add(
        "template/database/objects/properties/tableCompanies_Metadata.xml",
        b"<AccessObject><Type>Table</Type><Name>Companies</Name></AccessObject>",
    );
    add("template/database/objects/_rels/tableCompanies.xsd.rels", b"<Relationships><Relationship Id=\"a\" Type=\"http://schemas.microsoft.com/office/access/2005/04/template/table-data\" Target=\"sampleData/tableCompanies.xml\"/><Relationship Id=\"b\" Type=\"http://schemas.microsoft.com/office/access/2005/04/template/object-metadata\" Target=\"properties/tableCompanies_Metadata.xml\"/><Relationship Id=\"c\" Type=\"http://schemas.microsoft.com/office/2007/relationships/DataMacros\" Target=\"dataMacros/datamacrosCompanies.axl\"/></Relationships>");
    add(
        "template/database/objects/dataMacros/datamacrosCompanies.axl",
        &utf16(
            "<?xml version=\"1.0\" encoding=\"UTF-16\" standalone=\"no\"?><DataMacros xmlns=\"a\"><DataMacro Event=\"BeforeChange\"><Statements><Action Name=\"SetField\"><Argument Name=\"Field\">Active</Argument><Argument Name=\"Value\">1</Argument></Action></Statements></DataMacro></DataMacros>",
        ),
    );
    // Form named with a space, real name from metadata.
    add(
        "template/database/objects/formfrmMain.txt",
        &utf16(
            "Version =21\r\nBegin Form\r\n    RecordSource =\"Companies\"\r\n    Caption =\"Main \\\"window\\\" for\"\r\n        \" everything\"\r\n    Begin\r\n        Begin Section\r\n            Name =\"Detail\"\r\n            Begin\r\n                Begin CommandButton\r\n                    Name =\"cmdGo\"\r\n                    OnClick =\"[Event Procedure]\"\r\n                End\r\n            End\r\n        End\r\n    End\r\nEnd\r\nCodeBehindForm\r\nPrivate Sub cmdGo_Click()\r\nEnd Sub\r\n",
        ),
    );
    add(
        "template/database/objects/properties/formfrmMain_Metadata.xml",
        b"<AccessObject><Type>Form</Type><Name>Main Form</Name></AccessObject>",
    );
    add("template/database/objects/modulemodUtil.txt", b"\xEF\xBB\xBFOption Explicit\r\nPublic Function Hi() As String\r\n    Hi = \"hi\"\r\nEnd Function\r\n");
    add(
        "template/database/objects/properties/modulemodUtil_Metadata.xml",
        b"<AccessObject><Type>Module</Type><Name>modUtil</Name></AccessObject>",
    );
    add(
        "template/database/objects/macroAutoExec.txt",
        &utf16(
            "Version =196611\r\nBegin\r\n    Action =\"OpenForm\"\r\n    Argument =\"Main Form\"\r\nEnd\r\n",
        ),
    );
    add("template/database/objects/queryqryActive.txt", b"\xEF\xBB\xBFOperation =1\r\nOption =0\r\nWhere =\"Companies.Active=True\"\r\nBegin InputTables\r\n    Name =\"Companies\"\r\nEnd\r\nBegin OutputColumns\r\n    Expression =\"Companies.ID\"\r\nEnd\r\n");
    add("template/database/resources/Res1.png", b"PNG");
    add("template/database/resources/Res1-name.txt", &utf16("logo"));
    add("template/database/resources/_rels/Res1.png.rels", b"<Relationships><Relationship Id=\"name\" Type=\"http://schemas.microsoft.com/office/2007/relationships/resource-name\" Target=\"Res1-name.txt\"/></Relationships>");
    zip.finish().unwrap().into_inner()
}

#[test]
fn reads_every_part() {
    let pkg = Package::from_bytes(&build()).unwrap();
    assert_eq!(
        pkg.core_properties().unwrap().title.as_deref(),
        Some("Demo")
    );
    assert_eq!(
        pkg.template().unwrap().required_access_version.as_deref(),
        Some("14")
    );
    let props = pkg.database_properties().unwrap();
    assert!(
        props
            .iter()
            .any(|p| p.name == "StartUpForm" && p.value == "frmMain")
    );

    let kinds: Vec<(ObjectKind, &str)> = pkg
        .objects()
        .iter()
        .map(|o| (o.kind, o.name.as_str()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (ObjectKind::Table, "Companies"),
            (ObjectKind::Query, "qryActive"),
            (ObjectKind::Form, "Main Form"),
            (ObjectKind::Macro, "AutoExec"),
            (ObjectKind::Module, "modUtil")
        ]
    );

    let t = pkg.table("companies").unwrap();
    assert_eq!(
        t.columns.len(),
        4,
        "the Files column only exists in the data"
    );
    assert_eq!(t.columns[0].jet_type, JetType::AutoNumber);
    assert!(t.columns[0].auto_increment && t.columns[0].required);
    assert_eq!(t.columns[1].name, "Company Name");
    assert_eq!(t.columns[1].max_length, Some(50));
    assert_eq!(t.columns[1].description(), Some("Legal name"));
    assert_eq!(t.primary_key().unwrap().columns, vec!["ID"]);
    assert_eq!(t.rows[0][1], Some(Value::Text("Acme & Co".into())));
    match &t.rows[0][3] {
        Some(Value::Complex(records)) => {
            assert_eq!(records.len(), 2);
            assert_eq!(records[1]["FileName"], "b.png");
        }
        other => panic!("expected attachments, got {other:?}"),
    }
    assert_eq!(
        t.rows[1],
        vec![
            Some(Value::Text("2".into())),
            None,
            Some(Value::Text("0".into())),
            None
        ]
    );
    assert_eq!(
        t.to_csv(),
        "\"ID\",\"Company Name\",\"Active\",\"Files\"\n\"1\",\"Acme & Co\",\"1\",\"FileData=<4 base64 chars>; FileName=a.png | FileData=<4 base64 chars>; FileName=b.png\"\n\"2\",,\"0\",\n"
    );
    assert!(t.column("COMPANYID").is_none() && t.column("id").is_some());
    let dm = pkg.data_macros("Companies").unwrap();
    assert_eq!(dm[0].event, "BeforeChange");
    assert_eq!(
        dm[0].actions[0].arguments[0],
        ("Field".to_string(), "Active".to_string())
    );

    let f = pkg.form("Main Form").unwrap();
    assert_eq!(f.record_source(), Some("Companies"));
    assert_eq!(f.get("Caption"), Some("Main \"window\" for everything"));
    assert_eq!(
        f.controls().iter().map(|c| c.name()).collect::<Vec<_>>(),
        vec!["cmdGo"]
    );
    assert_eq!(
        f.events()[0].procedure_name().as_deref(),
        Some("cmdGo_Click")
    );
    assert!(f.code_behind().unwrap().contains("cmdGo_Click"));
    assert!(matches!(
        pkg.form("nope"),
        Err(accdt::Error::MissingObject { kind: "form", .. })
    ));

    assert_eq!(
        pkg.module("modUtil").unwrap().source.lines().next(),
        Some("Option Explicit")
    );
    let m = pkg.ui_macro("AutoExec").unwrap();
    assert_eq!(m.actions[0].arguments, vec!["Main Form"]);
    let q = pkg.query("qryActive").unwrap();
    assert_eq!(q.definition.operation, Some(Operation::Select));
    let sql = q.to_sql();
    assert_eq!(sql.source, SqlSource::Reconstructed);
    assert_eq!(
        sql.text,
        "SELECT Companies.ID\nFROM Companies\nWHERE Companies.Active=True\n;"
    );

    let rels = pkg.relationships().unwrap();
    assert_eq!(
        rels[0].columns,
        vec![("CompanyID".to_string(), "ID".to_string())]
    );
    assert!(rels[0].enforces_integrity() && rels[0].cascade_update() && rels[0].cascade_delete());
    let refs = pkg.vba_references().unwrap();
    assert_eq!(refs[0].known_name(), Some("Visual Basic For Applications"));
    assert!(refs[1].is_32bit_only());
    let res = pkg.resources().unwrap();
    assert_eq!(res[0].name, "logo");
    assert_eq!(
        pkg.template().unwrap().template_type.as_deref(),
        Some("Getting Started")
    );
    assert!(
        pkg.part("template/database/objects/tableCompanies.xsd")
            .is_some()
    );
}

#[test]
fn malformed_rels_are_errors_not_silence() {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut add = |name: &str, bytes: &[u8]| {
        zip.start_file(name, opts).unwrap();
        zip.write_all(bytes).unwrap();
    };
    add("template/template.xml", b"<Template/>");
    add("template/database/objects/tableT.xsd", b"<xsd:schema xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\"><xsd:element name=\"dataroot\"/><xsd:element name=\"T\"><xsd:complexType><xsd:sequence/></xsd:complexType></xsd:element></xsd:schema>");
    add("template/database/objects/_rels/tableT.xsd.rels", b"<Relationships><Relationship Id=\"a\" Type=\"http://schemas.microsoft.com/office/access/2005/04/template/table-data\" Target=\"./sampleData/tableT.xml\"/></Relationships>");
    let bytes = zip.finish().unwrap().into_inner();
    let pkg = Package::from_bytes(&bytes).unwrap();
    // Declared but missing data part is an error, not an empty table.
    assert!(matches!(pkg.table("T"), Err(accdt::Error::MissingPart(_))));

    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut add = |name: &str, bytes: &[u8]| {
        zip.start_file(name, opts).unwrap();
        zip.write_all(bytes).unwrap();
    };
    add("template/template.xml", b"<Template/>");
    add("template/database/objects/tableT.xsd", b"<xsd:schema/>");
    add(
        "template/database/objects/_rels/tableT.xsd.rels",
        b"<Relationships><Broken",
    );
    let bytes = zip.finish().unwrap().into_inner();
    assert!(matches!(
        Package::from_bytes(&bytes),
        Err(accdt::Error::Xml { .. })
    ));
}

#[test]
fn rejects_non_packages() {
    assert!(Package::from_bytes(b"not a zip").is_err());
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("hello.txt", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"hi").unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert!(matches!(
        Package::from_bytes(&bytes),
        Err(accdt::Error::NotAPackage(_))
    ));
}
