//! Access 2010 templates with SharePoint lists and AXL (web database) objects. These files
//! are not downloadable from Microsoft's CDN: they come out of the Access 2010 install media
//! (`AccLR.cab` entries `ACCESSTEMPLATE_107.ACCDT_1033`, `ACCESSTEMPLATE_1100.ACCDT_1033`,
//! `ACCESSTEMPLATE_CONTACTS.ACCDT_1033`), so the tests skip when they are absent.

use accdt::{ObjectKind, Package, PartFormat, SqlSource};

fn open(name: &str) -> Option<Package> {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::path::Path::new(&p)
        .exists()
        .then(|| Package::open(&p).unwrap())
}

#[test]
fn wss_tasks_template_links_sharepoint_lists() {
    let Some(pkg) = open("wss-107-tasks.accdt") else {
        return;
    };
    assert_eq!(pkg.objects_of(ObjectKind::Table).count(), 2);
    let tasks = pkg.table("Tasks").unwrap();
    let sp = tasks
        .sharepoint
        .as_ref()
        .expect("Tasks is a SharePoint list link");
    assert_eq!(sp.template_id, Some(107));
    assert!(sp.display_views_on_site);
    assert!(tasks.is_linked() && !tasks.has_data_part);
    let users = pkg.table("User Information List").unwrap();
    let sp = users.sharepoint.as_ref().unwrap();
    assert_eq!(sp.template_id, Some(112));
    assert_eq!(sp.root_folder.as_deref(), Some("_catalogs/users"));
    assert!(
        sp.default_view_url
            .as_deref()
            .is_some_and(|u| u.ends_with("detail.aspx"))
    );
    assert!(
        pkg.forms()
            .unwrap()
            .iter()
            .all(|f| !f.is_axl() && !f.controls().is_empty())
    );
}

#[test]
fn contacts_web_database_reads_axl_objects() {
    let Some(pkg) = open("access2010-contacts-web.accdt") else {
        return;
    };
    // Variations are grouped under their base object, not listed as objects.
    let names: Vec<&str> = pkg.objects().iter().map(|o| o.name.as_str()).collect();
    assert!(
        !names
            .iter()
            .any(|n| n.contains("_Flip") || n.contains("_AddFurigana") || n.starts_with("ooo")),
        "{names:?}"
    );
    assert_eq!(pkg.objects_of(ObjectKind::Table).count(), 2, "{names:?}");
    let contacts = pkg.object(ObjectKind::Table, "Contacts").unwrap();
    assert_eq!(contacts.variations.len(), 3);
    assert!(contacts.variations.iter().any(|v| v.id == "FlipName"));
    let t = pkg.table("Contacts").unwrap();
    assert_eq!(t.sharepoint.as_ref().and_then(|s| s.template_id), Some(105));
    let flipped = pkg.table_variation("Contacts", "FlipName").unwrap();
    assert!(!flipped.columns.is_empty());

    // AXL forms flow through the same Design views.
    let list = pkg.form("ContactList").unwrap();
    assert!(list.is_axl());
    assert_eq!(list.record_source(), Some("Contacts"));
    let ctrls = list.controls();
    assert!(
        ctrls.iter().any(|c| c.name == "txtContactName"
            && c.control_type == "TextBox"
            && c.get("ControlSource") == Some("ContactName")),
        "{:?}",
        ctrls.iter().map(|c| &c.name).collect::<Vec<_>>()
    );
    assert!(ctrls.iter().any(|c| c.name == "Detail" && c.is_section()));
    let macros = list.embedded_macros();
    assert!(!macros.is_empty(), "UI macros become embedded macros");
    assert!(list.events().iter().any(|e| e.value == "[Embedded Macro]"));
    let name_card = pkg.object(ObjectKind::Form, "NameCard").unwrap();
    assert_eq!(name_card.format, PartFormat::Axl);
    assert_eq!(name_card.variations.len(), 2);
    assert!(
        pkg.form_variation("NameCard", "FlipAddress")
            .unwrap()
            .controls()
            .len()
            > 1
    );

    // AXL reports and queries.
    let report = pkg.report("Comments").unwrap();
    assert_eq!(report.record_source(), Some("Comments"));
    assert!(
        report
            .controls()
            .iter()
            .any(|c| c.control_type == "Textbox" || c.is_section())
    );
    let q = pkg.query("ContactsExtended").unwrap();
    let sql = q.to_sql();
    assert_eq!(sql.source, SqlSource::Reconstructed);
    assert!(
        sql.text.contains("Contacts.*") && sql.text.contains("AS Searchable"),
        "{}",
        sql.text
    );

    // SharePoint list definitions.
    let lists = pkg.list_definitions().unwrap();
    assert_eq!(lists.len(), 2);
    let comments = lists.iter().find(|l| l.list_name == "Comments").unwrap();
    assert_eq!(comments.template_id, Some(100));
    assert!(
        comments
            .fields
            .iter()
            .any(|f| f.display_name == "CommentDate" && f.field_type == "DateTime" && f.added)
    );
    assert!(pkg.template().unwrap().variation_identifier.is_some());
}
