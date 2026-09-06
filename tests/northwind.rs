//! Runs against Microsoft's Northwind 2.0 Developer Edition template when present
//! (`scripts/fetch-fixtures.sh`).

use accdt::{JetType, ObjectKind, Package, SqlSource, Value};

mod common;

fn open() -> Option<Package> {
    common::required(common::NORTHWIND)
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
    assert_eq!(
        pkg.core_properties().unwrap().title.as_deref(),
        Some("Northwind dev edition")
    );
    assert!(
        pkg.database_properties()
            .unwrap()
            .iter()
            .any(|p| p.name == "AppTitle")
    );
    assert!(
        pkg.vba_references()
            .unwrap()
            .iter()
            .any(|r| r.known_name().is_some())
    );
}

#[test]
fn companies_table() {
    let Some(pkg) = open() else { return };
    let t = pkg.table("Companies").unwrap();
    assert_eq!(t.columns[0].name, "CompanyID");
    assert_eq!(t.columns[0].jet_type, JetType::AutoNumber);
    assert_eq!(t.column("CompanyName").unwrap().max_length, Some(50));
    assert_eq!(t.primary_key().unwrap().columns, vec!["CompanyID"]);
    assert_eq!(t.rows.len(), 13);
    assert_eq!(
        t.rows[0].cells[1].value().and_then(Value::as_str),
        Some("Adatum Corporation")
    );
    let dm = pkg.data_macros("Companies").unwrap();
    assert!(dm.iter().any(|m| m.event == "BeforeChange"));
    // Attachments are structured, not whitespace.
    let e = pkg.table("Employees").unwrap();
    let col = e
        .columns
        .iter()
        .position(|c| c.name == "Attachments")
        .unwrap();
    let photos = e
        .rows
        .iter()
        .filter(|r| matches!(r.cells[col].value(), Some(Value::Complex(_))))
        .count();
    assert!(photos >= 9, "{photos} employees with attachments");
    if let Some(Value::Complex(recs)) = e.rows[0].cells[col].value() {
        assert!(
            recs[0]
                .fields
                .iter()
                .find(|(n, _)| n == "FileName")
                .and_then(|(_, c)| c.value())
                .and_then(Value::as_str)
                .is_some_and(|n| n.ends_with(".jpg")),
            "{recs:?}"
        );
    }
}

#[test]
fn login_form_and_queries() {
    let Some(pkg) = open() else { return };
    let f = pkg.form("frmLogin").unwrap();
    let ctrls = f.controls();
    let login = ctrls.iter().find(|c| c.name() == "cmdLogin").unwrap();
    assert_eq!(login.kind(), accdt::ControlKind::CommandButton);
    assert!(
        login.layout().unwrap().left.is_some(),
        "{:?}",
        login.layout()
    );
    let filled = login.layout().unwrap();
    assert!(
        filled.left.is_some() && filled.width.is_some(),
        "{filled:?}"
    );
    assert!(
        f.events()
            .iter()
            .any(|e| e.owner == "cmdLogin" && e.event == "Click")
    );
    assert!(f.code_behind().unwrap().contains("Sub cmdLogin_Click"));
    assert!(f.control_defaults().contains_key("TextBox"));
    // Wrapped strings are joined and unescaped.
    let learn = pkg.form("frmLearn").unwrap();
    let learn_ctrls = learn.controls();
    let caption = learn_ctrls
        .iter()
        .find_map(|c| {
            c.raw_property("Caption")
                .filter(|v| v.starts_with("This form uses the new"))
                .map(String::from)
        })
        .unwrap();
    assert!(
        caption.contains(
            "older versions of Access. For those, please click the button to open an Access report"
        ),
        "{caption}"
    );
    assert!(caption.contains("\r\n"), "octal escapes decoded");
    assert!(
        learn.document().warnings.is_empty(),
        "{:?}",
        learn.document().warnings
    );
    let macros = learn.embedded_macros().unwrap();
    let open_report = &macros["cmdOpenReport.Click"];
    let report = open_report.entry().steps.iter().find_map(|s| match s {
        accdt::Step::Always(accdt::Action::OpenReport { report, .. }) => Some(report.as_str()),
        _ => None,
    });
    assert_eq!(report, Some("rptLearn"));
    assert!(
        pkg.forms()
            .unwrap()
            .iter()
            .chain(pkg.reports().unwrap().iter())
            .all(|d| d.document().warnings.is_empty())
    );
    // Bare event names.
    let list = pkg.form("frmEmployeeList").unwrap();
    assert!(
        list.events().iter().any(|e| e.event == "AfterUpdate"),
        "{:?}",
        list.events()
    );

    // Queries: continuation-safe, alias-aware, stored SQL used.
    let q = pkg.query("qryPurchaseOrderList").unwrap().to_sql();
    assert!(
        q.text.contains("Nz([PurchaseOrders].[TaxAmount])"),
        "{}",
        q.text
    );
    let q = pkg.query("qryVendorPurchaseOrderList").unwrap().to_sql();
    assert!(q.complete && q.source == SqlSource::Reconstructed);
    assert!(
        q.text.contains("qrycboEmployees AS SumbittedBy ON"),
        "{}",
        q.text
    );
    assert!(
        !q.text.contains(", qrycboEmployees"),
        "no cartesian leftovers: {}",
        q.text
    );
    let q = pkg.query("qrycboProductCategories").unwrap().to_sql();
    assert_eq!(q.source, SqlSource::Stored);
    assert!(
        q.text.contains("UNION ALL") && q.text.contains("\"<All>\""),
        "{}",
        q.text
    );
    let q = pkg.query("qryOrders_MostRecent").unwrap().to_sql();
    assert!(q.text.starts_with("SELECT TOP 20"), "{}", q.text);
    let queries = pkg.queries().unwrap();
    let executable = queries
        .iter()
        .filter(|q| q.to_sql().is_executable())
        .count();
    assert!(
        executable >= 63,
        "{executable} of {} queries executable",
        queries.len()
    );
    assert!(
        queries
            .iter()
            .all(|q| accdt::saveastext::parse(&q.text).warnings.is_empty())
    );

    let m = pkg.ui_macro("AutoExec").unwrap();
    assert!(
        m.entry()
            .steps
            .iter()
            .any(|s| matches!(s, accdt::Step::When { body, .. } if matches!(body.first(), Some(accdt::Action::OpenForm { .. })))),
        "{:?}",
        m.entry().steps
    );
    assert!(
        pkg.reports()
            .unwrap()
            .iter()
            .all(|r| !r.controls().is_empty())
    );
    assert!(!pkg.resources().unwrap().is_empty());
    assert!(
        pkg.relationships()
            .unwrap()
            .iter()
            .any(|r| r.enforces_integrity())
    );
}

#[test]
fn desktop_group_sections_match_nested_levels() {
    let Some(pkg) = open() else { return };
    let mut nested = 0;
    for report in pkg.reports().unwrap() {
        let groups = report.group_levels().unwrap();
        if groups.iter().filter(|g| g.header).count() > 1 {
            nested += 1;
        }
        for group in groups {
            assert!(
                group.diagnostics.is_empty(),
                "{}: {:?}",
                report.name(),
                group.diagnostics
            );
            assert_eq!(group.header, group.header_section.is_some());
            assert_eq!(group.footer, group.footer_section.is_some());
        }
    }
    assert!(nested > 0, "Northwind must provide nested header evidence");
}
