//! Lexer / Pratt / SQLite renderer: one case per Access quirk.

use accdt::expr::{Expr, Param, parse_control_source, parse_expr, parse_ident_path};

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
