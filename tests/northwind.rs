//! Runs against Microsoft's Northwind 2.0 Developer Edition template when present
//! (`scripts/fetch-fixtures.sh`).

use accdt::{JetType, ObjectKind, Package};

fn open() -> Option<Package> {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/northwind-2.0-dev.accdt");
    std::path::Path::new(p).exists().then(|| Package::open(p).unwrap())
}

#[test]
fn object_counts() {
    let Some(pkg) = open() else { return };
    let count = |k| pkg.objects_of(k).count();
    assert_eq!(count(ObjectKind::Table), 28);
    assert_eq!(count(ObjectKind::Query), 65);
    assert_eq!(count(ObjectKind::Form), 40);
    assert_eq!(count(ObjectKind::Report), 15);
    assert_eq!(count(ObjectKind::Macro), 1);
    assert_eq!(count(ObjectKind::Module), 18);
    assert_eq!(pkg.relationships().unwrap().len(), 27);
    assert_eq!(pkg.core_properties().unwrap().title.as_deref(), Some("Northwind dev edition"));
    assert!(pkg.database_properties().unwrap().iter().any(|p| p.name == "AppTitle"));
    assert!(pkg.vba_references().unwrap().iter().any(|r| r.known_name().is_some()));
}

#[test]
fn companies_table() {
    let Some(pkg) = open() else { return };
    let t = pkg.table("Companies").unwrap().unwrap();
    assert_eq!(t.columns[0].name, "CompanyID");
    assert_eq!(t.columns[0].jet_type, JetType::AutoNumber);
    assert_eq!(t.column("CompanyName").unwrap().max_length, Some(50));
    assert_eq!(t.primary_key().unwrap().columns, vec!["CompanyID"]);
    assert_eq!(t.rows.len(), 13);
    assert_eq!(t.rows[0][1].as_deref(), Some("Adatum Corporation"));
    let dm = pkg.data_macros(pkg.object(ObjectKind::Table, "Companies").unwrap()).unwrap();
    assert!(dm.iter().any(|m| m.event == "BeforeChange"));
}

#[test]
fn login_form_and_queries() {
    let Some(pkg) = open() else { return };
    let f = pkg.form("frmLogin").unwrap().unwrap();
    let ctrls = f.controls();
    let login = ctrls.iter().find(|c| c.name == "cmdLogin").unwrap();
    assert_eq!(login.control_type, "CommandButton");
    assert!(login.layout().left.is_some() && login.layout().height.is_none(), "{:?}", login.layout());
    assert!(f.layout(login).height.is_some(), "height must come from the design defaults");
    assert!(f.events().iter().any(|e| e.owner == "cmdLogin" && e.event == "Click"));
    assert!(f.code_behind.as_ref().unwrap().contains("Sub cmdLogin_Click"));
    assert!(f.control_defaults.contains_key("TextBox"));
    let queries = pkg.queries().unwrap();
    let complete = queries.iter().filter(|q| q.to_sql().1).count();
    assert!(complete >= 60, "{complete} of {} queries reconstructed", queries.len());
    let m = &pkg.macros().unwrap()[0];
    assert_eq!(m.name, "AutoExec");
    assert_eq!(m.actions[0].action, "OpenForm");
    assert!(pkg.reports().unwrap().iter().all(|r| !r.controls().is_empty()));
    assert!(!pkg.resources().is_empty());
}
