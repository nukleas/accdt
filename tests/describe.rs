//! Goldens on the Projects template for the descriptions a port reads first, and for typed
//! sample-data cells (Filters rows here; dates and currency from Northwind 2.0).
use accdt::{Cell, DesignDescription, DesignKind, Package, TableDescription, Value};

fn projects() -> Option<Package> {
    Package::open("tests/fixtures/project-management.accdt").ok()
}

#[test]
fn project_details_description() {
    let Some(pkg) = projects() else { return };
    let d = DesignDescription::new(&pkg.form("Project Details").unwrap(), Some(&pkg), false);
    assert_eq!(d.kind, DesignKind::Form);
    assert_eq!(d.default_view.as_deref(), Some("single"));
    assert_eq!(
        (
            d.record_source.classification.as_str(),
            d.record_source.raw.as_str()
        ),
        ("table", "Projects")
    );
    assert_eq!(
        d.sections
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["FormHeader", "Detail", "FormFooter"]
    );
    // Attached labels fold into their control; layout placeholders are gone.
    let detail = &d.sections[1];
    assert!(
        detail
            .controls
            .iter()
            .all(|c| !c.name.contains("_LayoutLabel"))
    );
    let owner = detail.controls.iter().find(|c| c.name == "Owner").unwrap();
    assert_eq!(owner.label.as_deref(), Some("Owner"));
    assert_eq!(owner.depth, 2, "inside the tab control's page");
    let lookup = owner.lookup.as_ref().unwrap();
    assert_eq!(
        (
            lookup.kind.as_str(),
            lookup.column_count,
            lookup.display_column
        ),
        ("sql", 2, Some(2))
    );
    let status = detail.controls.iter().find(|c| c.name == "Status").unwrap();
    assert_eq!(
        status.lookup.as_ref().unwrap().values,
        [
            "Not Started",
            "In Progress",
            "Completed",
            "Deferred",
            "Waiting on someone else"
        ]
    );
    let sub = detail
        .controls
        .iter()
        .find(|c| c.name == "Tasks subform")
        .unwrap();
    let link = sub.subform.as_ref().unwrap();
    assert_eq!(
        (link.target.as_str(), link.resolution.as_str()),
        ("Form.Tasks subform", "resolved")
    );
    assert_eq!(link.links, [("ID".to_string(), "Project".to_string())]);
    // Form.Load is an embedded macro; buttons carry theirs.
    assert!(d.events.iter().any(|e| {
        e.event == "Load"
            && e.actions
                .iter()
                .any(|a| a.starts_with("If Not IsNull([OpenArgs]): GoToRecord"))
    }));
    let print = d.sections[0]
        .controls
        .iter()
        .find(|c| c.name == "cmdPrint")
        .unwrap();
    assert_eq!(print.events[0].actions, ["OpenReport(Project Tasks, view=Report, window=Normal)"]);
    let footer = &d.sections[2];
    let sum = footer
        .controls
        .iter()
        .find(|c| c.name == "SumTaskCost")
        .unwrap();
    assert_eq!(
        (
            sum.control_source.as_deref(),
            sum.format.as_deref(),
            sum.locked
        ),
        (Some("=[Tasks subform]![SumOfCost]"), Some("Currency"), true)
    );
    assert!(d.diagnostics.is_empty(), "{:?}", d.diagnostics);
    let text = d.to_string();
    assert!(
        text.starts_with("form Project Details — single\nrecord source: table Projects\n"),
        "{text}"
    );
    assert!(text.contains("link: master ID -> child Project"));
}

#[test]
fn split_form_and_all_controls_option() {
    let Some(pkg) = projects() else { return };
    let form = pkg.form("Project List").unwrap();
    let d = DesignDescription::new(&form, Some(&pkg), false);
    assert_eq!(d.default_view.as_deref(), Some("split"));
    assert_eq!(d.record_source.classification, "query");
    let all = DesignDescription::new(&form, Some(&pkg), true);
    let n = |d: &DesignDescription| d.sections.iter().map(|s| s.controls.len()).sum::<usize>();
    assert!(
        n(&all) > n(&d) + 10,
        "labels are back with --all-controls: {} vs {}",
        n(&all),
        n(&d)
    );
    let task = pkg.form("Task Details").unwrap();
    let d = DesignDescription::new(&task, Some(&pkg), false);
    assert_eq!(d.record_source.classification, "sql");
    let hidden = d.sections[1]
        .controls
        .iter()
        .find(|c| c.name == "Project")
        .unwrap();
    assert!(
        !hidden.visible
            && hidden.default_value.as_deref() == Some("=[Forms]![Project Details]![ID]")
    );
}

#[test]
fn report_groups_and_sorts() {
    let Some(pkg) = projects() else { return };
    let d = DesignDescription::new(
        &pkg.report("Employee Address Book").unwrap(),
        Some(&pkg),
        false,
    );
    assert_eq!(d.groups.len(), 2);
    let g = &d.groups[0];
    assert_eq!(
        (
            g.control_source.as_str(),
            g.header,
            g.footer,
            g.group_on.as_str(),
            g.header_section.as_deref()
        ),
        (
            "File As",
            true,
            false,
            "PrefixCharacters",
            Some("GroupHeader3")
        )
    );
    assert_eq!(g.keep_together, "WithFirstDetail");
    assert!(!d.groups[1].header && d.groups[1].group_on == "EachValue");
    let d = DesignDescription::new(
        &pkg.report("Tasks by Assigned To").unwrap(),
        Some(&pkg),
        false,
    );
    assert_eq!(
        d.groups
            .iter()
            .map(|g| (g.control_source.as_str(), g.header, g.footer))
            .collect::<Vec<_>>(),
        [
            ("Assigned To", true, true),
            ("Priority", false, false),
            ("Due Date", false, false)
        ]
    );
    assert_eq!(d.groups[0].footer_section.as_deref(), Some("GroupFooter0"));
    let text = d.to_string();
    assert!(
        text.contains(
            "group 0: Assigned To; EachValue ascending; header GroupHeader0; footer GroupFooter0"
        ),
        "{text}"
    );
    assert!(text.contains("sort 1: Priority;"));
}

#[test]
fn table_description_and_typed_cells() {
    let Some(pkg) = projects() else { return };
    let t = pkg.table("Common Tasks").unwrap();
    let d = TableDescription::new(&t, Some(&pkg));
    let sp = d.sharepoint.as_ref().unwrap();
    assert!(!sp.linked && sp.template_id == Some(107));
    assert!(d.primary_key.is_empty(), "Common Tasks has no primary key");
    let priority = d.columns.iter().find(|c| c.name == "Priority").unwrap();
    assert_eq!(
        priority.lookup.as_ref().unwrap().values,
        ["(1) High", "(2) Normal", "(3) Low"]
    );
    let assigned = d.columns.iter().find(|c| c.name == "Assigned To").unwrap();
    assert_eq!(assigned.lookup.as_ref().unwrap().display_column, Some(2));
    assert_eq!(
        d.relationships,
        ["Common Tasks.Assigned To -> Employees.ID"]
    );
    let text = d.to_string();
    assert!(
        text.starts_with("table Common Tasks — local (publishable as list template 107)\n"),
        "{text}"
    );
    assert!(text.contains("% Complete Double default 0 rule <=1 And >=0 [Percent]"));

    let filters = pkg.table("Filters").unwrap();
    assert_eq!(filters.rows.len(), 3);
    let row = &filters.rows[0].cells;
    let col = |n: &str| filters.columns.iter().position(|c| c.name == n).unwrap();
    assert_eq!(row[col("ID")].value(), Some(&Value::Integer(1)));
    assert_eq!(row[col("Object Type")].value(), Some(&Value::Integer(2)));
    assert_eq!(row[col("Default")].value(), Some(&Value::Boolean(false)));
    assert_eq!(
        row[col("Filter String")].value(),
        Some(&Value::Text(
            "([Open Projects].[Status]=\"In Progress\")".into()
        ))
    );
    assert!(matches!(row[col("Description")], Cell::Null { .. }));
    assert!(filters.validate_values().is_ok());
    assert_eq!(
        TableDescription::new(&filters, Some(&pkg)).primary_key,
        ["ID"]
    );
}

#[test]
fn northwind_dates_and_currency_decode() {
    let Ok(pkg) = Package::open("tests/fixtures/northwind-2.0-dev.accdt") else {
        return;
    };
    let t = pkg.table("PurchaseOrders").unwrap();
    assert!(t.validate_values().is_ok());
    let col = |n: &str| t.columns.iter().position(|c| c.name == n).unwrap();
    let Some(Value::DateTime(d)) = t.rows[0].cells[col("SubmittedDate")].value() else {
        panic!()
    };
    assert_eq!(
        (d.year, d.month, d.day, d.hour, d.minute),
        (2023, 9, 29, 8, 0)
    );
    assert_eq!(
        t.rows[0].cells[col("SubmittedDate")].lexical(),
        Some("2023-09-29T08:00:00")
    );
    let Some(Value::Currency(c)) = t.rows[0].cells[col("TaxAmount")].value() else {
        panic!()
    };
    assert_eq!(c.to_string(), "5009.9900");
    let Some(Value::Currency(fee)) = t.rows[0].cells[col("ShippingFee")].value() else {
        panic!()
    };
    assert_eq!(fee.scaled(), 100 * 10_000);
}
