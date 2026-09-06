//! Lexer / Pratt / SQLite renderer: one case per Access quirk, plus Projects goldens.

use accdt::expr::{
    Expr, Param, parse_control_source, parse_expr, parse_filter_string, parse_ident_path,
};
use accdt::{Package, SqlSource, Value};

fn sqlite(s: &str) -> String {
    parse_expr(s)
        .unwrap_or_else(|e| panic!("{s:?}: {e}"))
        .to_sqlite()
}

fn params(s: &str) -> Vec<Param> {
    parse_expr(s).unwrap().parameters()
}

#[test]
fn double_quoted_strings_are_sql_strings() {
    assert_eq!(sqlite(r#""Completed""#), "'Completed'");
    assert_eq!(sqlite(r#""a ""quoted"" word""#), "'a \"quoted\" word'");
    assert_eq!(sqlite(r#""(1) High""#), "'(1) High'");
}

#[test]
fn bracket_identifiers() {
    assert_eq!(sqlite("[Last Name]"), r#""Last Name""#);
    assert_eq!(sqlite("[Budget in Days]"), r#""Budget in Days""#);
    assert_eq!(sqlite("[Name]]s]"), r#""Name]s""#);
}

#[test]
fn qualified_and_star() {
    assert_eq!(sqlite("Projects.Status"), r#""Projects"."Status""#);
    assert_eq!(
        sqlite("[Open Projects].[Status]"),
        r#""Open Projects"."Status""#
    );
    assert_eq!(sqlite("Employees.*"), r#""Employees".*"#);
    assert_eq!(sqlite("Count(*)"), "COUNT(*)");
}

#[test]
fn concat_ampersand() {
    assert_eq!(
        sqlite(r#"[Last Name] & ", " & [First Name]"#),
        r#""Last Name" || ', ' || "First Name""#
    );
}

#[test]
fn bang_path_with_spaces_is_a_form_param() {
    let e = parse_expr("Forms![Project Details]!ID").unwrap();
    assert_eq!(e.to_sqlite(), ":forms_Project_Details_ID");
    assert_eq!(
        e.parameters(),
        vec![Param::Form {
            form: "Project Details".into(),
            control: "ID".into(),
        }]
    );
}

#[test]
fn hash_date_literals() {
    assert_eq!(sqlite("#1/1/1900#"), "date('1900-01-01')");
    assert_eq!(
        sqlite("#2010-01-01 12:00:00#"),
        "datetime('2010-01-01 12:00:00')"
    );
}

#[test]
fn true_false_null() {
    assert_eq!(sqlite("True"), "1");
    assert_eq!(sqlite("False"), "0");
    assert_eq!(sqlite("Null"), "NULL");
    assert_eq!(sqlite("-1"), "-1");
}

#[test]
fn like_wildcards() {
    assert_eq!(sqlite(r#"[Name] Like "a*""#), r#""Name" LIKE 'a%'"#);
    assert_eq!(sqlite(r#"[Name] Like "a?""#), r#""Name" LIKE 'a_'"#);
    assert_eq!(
        sqlite(r#"[Name] Like "100%""#),
        r#""Name" LIKE '100\%' ESCAPE '\'"#
    );
}

#[test]
fn between_in_is_null() {
    assert_eq!(sqlite("[x] Between 1 And 10"), r#""x" BETWEEN 1 AND 10"#);
    assert_eq!(
        sqlite(r#"[Status] In ("Completed","Deferred")"#),
        r#""Status" IN ('Completed', 'Deferred')"#
    );
    assert_eq!(sqlite("[x] Is Null"), r#""x" IS NULL"#);
    assert_eq!(sqlite("[x] Is Not Null"), r#""x" IS NOT NULL"#);
}

#[test]
fn iif_isnull_nz_ccur() {
    assert_eq!(
        sqlite("IIf(IsNull([Last Name]),[Company],[Last Name])"),
        r#"CASE WHEN "Last Name" IS NULL THEN "Company" ELSE "Last Name" END"#
    );
    assert_eq!(sqlite("Nz([Budget],0)"), r#"COALESCE("Budget", 0)"#);
    assert_eq!(sqlite("Nz([File As])"), r#"COALESCE("File As", '')"#);
    assert_eq!(
        sqlite("CCur(nz(Sum([Tasks].[Cost]),0))"),
        r#"CAST(COALESCE(SUM("Tasks"."Cost"), 0) AS REAL)"#
    );
}

#[test]
fn nested_iif_employees_extended() {
    let src = r#"IIf(IsNull([Last Name]),IIf(IsNull([First Name]),[Company],[First Name]),IIf(IsNull([First Name]),[Last Name],[Last Name] & ", " & [First Name]))"#;
    assert_eq!(
        sqlite(src),
        r#"CASE WHEN "Last Name" IS NULL THEN CASE WHEN "First Name" IS NULL THEN "Company" ELSE "First Name" END ELSE CASE WHEN "First Name" IS NULL THEN "Last Name" ELSE "Last Name" || ', ' || "First Name" END END"#
    );
}

#[test]
fn leading_equals_is_stripped() {
    assert_eq!(
        sqlite("=[Budget]-[SumTaskCost]"),
        r#""Budget" - "SumTaskCost""#
    );
    assert_eq!(sqlite("=Nz([Budget],0)"), r#"COALESCE("Budget", 0)"#);
    assert_eq!(sqlite("=True"), "1");
}

#[test]
fn dlookup_nested_quotes() {
    let src = r#"DLookUp("[Filter String]","Filters","ID = " & [Screen].[ActiveControl])"#;
    let e = parse_expr(src).unwrap();
    match &e {
        Expr::Call { name, args } => {
            assert_eq!(name, "DLookUp");
            assert_eq!(args.len(), 3);
            assert_eq!(args[0], Expr::Text("[Filter String]".into()));
            assert_eq!(args[1], Expr::Text("Filters".into()));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        e.to_sqlite(),
        r#"DLookUp('[Filter String]', 'Filters', 'ID = ' || :screen_ActiveControl)"#
    );
    assert_eq!(params(src), vec![Param::Screen("ActiveControl".into())]);
}

#[test]
fn tempvars_parent_form_screen_subform() {
    assert_eq!(sqlite("[TempVars]![FilterString]"), ":tempvar_FilterString");
    assert_eq!(sqlite("[Parent]![VendorID]"), ":parent_VendorID");
    assert_eq!(
        sqlite("[Form]![cboFilterFavorites]"),
        ":control_cboFilterFavorites"
    );
    assert_eq!(sqlite("Form![ID]"), ":control_ID");
    assert_eq!(
        sqlite("[Tasks subform]![SumOfCost]"),
        ":control_Tasks_subform_SumOfCost"
    );
    assert_eq!(
        params("[Tasks subform]![SumOfCost]"),
        vec![Param::Control("Tasks subform!SumOfCost".into())]
    );
}

#[test]
fn ucase_left_nz_phone_list() {
    assert_eq!(
        sqlite("=UCase(Left(Nz([File As]),1))"),
        r#"upper(substr(COALESCE("File As", ''), 1, 1))"#
    );
}

#[test]
fn data_macro_idents() {
    assert_eq!(sqlite("[IsInsert]"), r#""IsInsert""#);
    assert_eq!(sqlite("[Old].[AddedBy]"), r#""Old"."AddedBy""#);
}

#[test]
fn and_or_not_mod_int_div_pow() {
    assert_eq!(
        sqlite(r#"(Projects.Status)<>"Completed" And (Projects.Status)<>"Deferred""#),
        r#""Projects"."Status" <> 'Completed' AND "Projects"."Status" <> 'Deferred'"#
    );
    assert_eq!(sqlite("Not [A] Like \"x*\""), r#"NOT "A" LIKE 'x%'"#);
    assert_eq!(sqlite("7 Mod 3"), "7 % 3");
    assert_eq!(sqlite("7 \\ 2"), "CAST(7 AS INTEGER) / CAST(2 AS INTEGER)");
    assert_eq!(sqlite("2 ^ 3"), "power(2, 3)");
}

#[test]
fn parse_errors_are_invalid_not_silent() {
    let err = parse_expr("1 +").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("expr"), "{msg}");
    assert!(parse_expr("").is_err());
    assert!(parse_expr("\"unterminated").is_err());
    assert!(parse_expr("[unterminated").is_err());
    assert!(parse_expr("#2010-01-01").is_err());
}

#[test]
fn control_source_and_ident_path() {
    let field = parse_control_source("Last Name").unwrap();
    assert_eq!(
        field,
        Expr::Ident {
            name: "Last Name".into(),
            quoted: false,
        }
    );
    let expr = parse_control_source("=Nz([Budget],0)").unwrap();
    assert_eq!(expr.to_sqlite(), r#"COALESCE("Budget", 0)"#);
    let path = parse_ident_path("[Employees Extended].[Employee Name]").unwrap();
    assert_eq!(path.to_sqlite(), r#""Employees Extended"."Employee Name""#);
}

#[test]
fn replace_and_date_time() {
    assert_eq!(
        sqlite(r#"Replace("Add Common Tasks To: |","|",Nz([Title],""))"#),
        "replace('Add Common Tasks To: |', '|', COALESCE(\"Title\", ''))"
    );
    assert_eq!(sqlite("Date()"), "date('now')");
    assert_eq!(sqlite("Time()"), "time('now')");
}

fn open_fixture(name: &str) -> Option<Package> {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    if !std::path::Path::new(&p).exists() {
        if std::env::var_os("ACCDT_SKIP_FIXTURE_TESTS").is_some() {
            return None;
        }
        panic!(
            "fixture {p} is missing; run scripts/fetch-fixtures.sh (or set ACCDT_SKIP_FIXTURE_TESTS=1)"
        );
    }
    Some(Package::open(&p).unwrap())
}

fn projects() -> Option<Package> {
    open_fixture("project-management.accdt")
}

#[test]
fn projects_employees_extended() {
    let Some(pkg) = projects() else { return };
    let sql = pkg
        .query("Employees Extended")
        .unwrap()
        .to_sqlite()
        .unwrap();
    assert_eq!(sql.source, SqlSource::Reconstructed);
    assert!(sql.complete, "{}", sql.text);
    assert!(
        sql.text.contains(
            r#"CASE WHEN "Last Name" IS NULL THEN CASE WHEN "First Name" IS NULL THEN "Company" ELSE "First Name" END ELSE CASE WHEN "First Name" IS NULL THEN "Last Name" ELSE "Last Name" || ', ' || "First Name" END END"#
        ),
        "{}",
        sql.text
    );
    assert!(
        sql.text.contains(r#""First Name" || ' ' || "Last Name""#),
        "{}",
        sql.text
    );
    assert!(sql.text.contains("ORDER BY"), "{}", sql.text);
    assert!(sql.text.contains(r#""Employees".*"#), "{}", sql.text);
}

#[test]
fn projects_project_totals() {
    let Some(pkg) = projects() else { return };
    let sql = pkg.query("Project Totals").unwrap().to_sqlite().unwrap();
    assert!(
        sql.text
            .contains(r#"CAST(COALESCE(SUM("Tasks"."Cost"), 0) AS REAL)"#),
        "{}",
        sql.text
    );
    assert!(
        sql.text
            .contains(r#"CAST(COALESCE(SUM("Tasks"."Cost In Days"), 0) AS REAL)"#),
        "{}",
        sql.text
    );
}

#[test]
fn projects_common_tasks_append_and_update() {
    let Some(pkg) = projects() else { return };
    let ins = pkg
        .query("Common Tasks Append")
        .unwrap()
        .to_sqlite()
        .unwrap();
    assert!(
        ins.text.contains(":forms_Project_Details_ID"),
        "{}",
        ins.text
    );
    assert!(
        ins.text.contains(r#""Common Tasks"."Add" = 1"#),
        "{}",
        ins.text
    );
    let upd = pkg
        .query("Common Tasks Update Add Field")
        .unwrap()
        .to_sqlite()
        .unwrap();
    assert!(upd.text.contains("= 0"), "{}", upd.text);
    assert!(
        upd.text.contains(r#""Common Tasks"."Add" = 1"#),
        "{}",
        upd.text
    );
}

#[test]
fn projects_open_and_completed_queries() {
    let Some(pkg) = projects() else { return };
    let open = pkg.query("Open Projects").unwrap().to_sqlite().unwrap();
    assert!(open.text.contains("'Completed'"), "{}", open.text);
    assert!(open.text.contains("'Deferred'"), "{}", open.text);
    assert!(!open.text.contains("\"Completed\""), "{}", open.text);
    assert!(open.text.contains(r#""End Date""#), "{}", open.text);
    let done = pkg
        .query("Completed and Deferred Projects")
        .unwrap()
        .to_sqlite()
        .unwrap();
    assert!(done.text.contains("'Completed'"), "{}", done.text);
    assert!(done.text.contains("'Deferred'"), "{}", done.text);
    let tasks = pkg.query("Open Tasks").unwrap().to_sqlite().unwrap();
    assert!(
        tasks.text.contains(r#""Tasks"."Status" <> 'Completed'"#),
        "{}",
        tasks.text
    );
}

#[test]
fn projects_filters_rows() {
    let Some(pkg) = projects() else { return };
    let t = pkg.table("Filters").unwrap();
    let col = t
        .columns
        .iter()
        .position(|c| c.name == "Filter String")
        .expect("Filter String");
    let strings: Vec<&str> = t
        .rows
        .iter()
        .filter_map(|r| r[col].as_ref().and_then(Value::as_str))
        .collect();
    assert!(strings.len() >= 3, "{strings:?}");
    let s0 = parse_filter_string(strings[0]).unwrap().to_sqlite();
    let s1 = parse_filter_string(strings[1]).unwrap().to_sqlite();
    let s2 = parse_filter_string(strings[2]).unwrap().to_sqlite();
    assert_eq!(s0, r#""Open Projects"."Status" = 'In Progress'"#);
    assert_eq!(s1, r#""Open Projects"."Status" = 'Not Started'"#);
    assert_eq!(s2, r#""Open Projects"."Priority" = '(1) High'"#);
}

#[test]
fn projects_project_details_control_sources() {
    let Some(pkg) = projects() else { return };
    let f = pkg.form("Project Details").unwrap();
    let sqlite_of = |raw: &str| {
        f.controls()
            .iter()
            .find(|c| c.get("ControlSource") == Some(raw))
            .unwrap_or_else(|| panic!("no control with ControlSource {raw}"))
            .control_source_expr()
            .unwrap()
            .unwrap()
            .to_sqlite()
    };
    assert_eq!(sqlite_of("=Nz([Budget],0)"), r#"COALESCE("Budget", 0)"#);
    assert_eq!(
        sqlite_of("=[Budget]-[SumTaskCost]"),
        r#""Budget" - "SumTaskCost""#
    );
    let sub = sqlite_of("=[Tasks subform]![SumOfCost]");
    assert_eq!(sub, ":control_Tasks_subform_SumOfCost");
    let ctrls = f.controls();
    let rs = ctrls
        .iter()
        .find_map(|c| {
            c.get("RowSource")
                .filter(|s| s.contains("Open Projects") && s.contains("Form![ID]"))
        })
        .expect("Open Projects RowSource");
    let where_e = parse_expr("[ID]<>Nz(Form![ID],0)").unwrap();
    assert!(rs.contains("[ID]<>Nz(Form![ID],0)"), "{rs}");
    assert_eq!(where_e.to_sqlite(), r#""ID" <> COALESCE(:control_ID, 0)"#);
    assert_eq!(where_e.parameters(), vec![Param::Control("ID".into())]);
}

#[test]
fn projects_task_details_default_value() {
    let Some(pkg) = projects() else { return };
    let f = pkg.form("Task Details").unwrap();
    let e = f
        .controls()
        .iter()
        .find_map(|c| {
            c.get("DefaultValue")
                .filter(|v| v.contains("Project Details"))
                .map(|_| c.default_value_expr().unwrap().unwrap())
        })
        .expect("DefaultValue Forms!Project Details");
    assert_eq!(e.to_sqlite(), ":forms_Project_Details_ID");
    assert_eq!(
        e.parameters(),
        vec![Param::Form {
            form: "Project Details".into(),
            control: "ID".into(),
        }]
    );
}

#[test]
fn projects_employee_phone_list_and_tasks_subreport() {
    let Some(pkg) = projects() else { return };
    let phone = pkg.report("Employee Phone List").unwrap();
    let letter = phone
        .controls()
        .iter()
        .find(|c| c.get("ControlSource") == Some("=UCase(Left(Nz([File As]),1))"))
        .expect("UCase Left File As")
        .control_source_expr()
        .unwrap()
        .unwrap()
        .to_sqlite();
    assert_eq!(letter, r#"upper(substr(COALESCE("File As", ''), 1, 1))"#);
    let sub = pkg.report("Tasks Subreport").unwrap();
    let cost = sub
        .controls()
        .iter()
        .find(|c| c.get("ControlSource") == Some("=Nz(Sum([Cost]),0)"))
        .expect("Nz Sum Cost")
        .control_source_expr()
        .unwrap()
        .unwrap()
        .to_sqlite();
    assert_eq!(cost, r#"COALESCE(SUM("Cost"), 0)"#);
}

#[test]
fn marketing_queries_translate() {
    let Some(pkg) = open_fixture("marketing-projects.accdt") else {
        return;
    };
    for q in pkg.queries().unwrap() {
        let access = q.to_sql();
        if access.source != SqlSource::Reconstructed {
            continue;
        }
        let sql = q.to_sqlite().unwrap_or_else(|e| panic!("{}: {e}", q.name));
        assert_eq!(sql.source, SqlSource::Reconstructed);
        assert!(
            sql.text.starts_with("SELECT")
                || sql.text.starts_with("INSERT")
                || sql.text.starts_with("UPDATE")
                || sql.text.starts_with("DELETE"),
            "{}: {}",
            q.name,
            sql.text
        );
    }
}

#[test]
fn northwind_stored_sql_is_not_translated() {
    let Some(pkg) = open_fixture("northwind-2.0-dev.accdt") else {
        return;
    };
    let q = pkg.query("qrycboProductCategories").unwrap();
    let sql = q.to_sqlite().unwrap();
    assert_eq!(sql.source, SqlSource::Stored);
    assert!(
        sql.text.contains("UNION ALL") && sql.text.contains("\"<All>\""),
        "{}",
        sql.text
    );
}
