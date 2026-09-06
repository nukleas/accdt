//! Macro tree: submacros, `...` grouping, typed actions, Projects + AutoExec goldens.

use accdt::expr::{Expr, parse_expr};
use accdt::{
    AcCmd, Action, ErrorNext, FormDataMode, FormView, Macro, ObjectType, Package, Step, WindowMode,
};

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

fn is_axl_comment(s: &Step) -> bool {
    matches!(s, Step::Always(Action::Comment(c)) if c.starts_with("_AXL:"))
}

#[test]
fn macroname_only_header_then_actions() {
    let m = Macro::parse(
        "M",
        "Version =196611\nBegin\n    MacroName =\"Go\"\nEnd\nBegin\n    Action =\"Beep\"\nEnd\nBegin\n    Action =\"StopMacro\"\nEnd\n"
            .into(),
    )
    .unwrap();
    assert_eq!(m.submacros.len(), 1);
    assert_eq!(m.submacros[0].name.as_deref(), Some("Go"));
    assert!(matches!(
        &m.submacros[0].steps[..],
        [Step::Always(Action::Beep), Step::Always(Action::StopMacro)]
    ));
}

#[test]
fn macroname_and_action_on_same_begin() {
    let m = Macro::parse(
        "M",
        "Begin\n    MacroName =\"SetLastFilterID\"\n    Action =\"SetTempVar\"\n    Argument =\"LastFilterCreated\"\n    Argument =\"[ID]\"\nEnd\n"
            .into(),
    )
    .unwrap();
    assert_eq!(m.submacros.len(), 1);
    assert_eq!(m.submacros[0].name.as_deref(), Some("SetLastFilterID"));
    match &m.submacros[0].steps[..] {
        [Step::Always(Action::SetTempVar { name, value })] => {
            assert_eq!(name, "LastFilterCreated");
            assert_eq!(
                *value,
                Expr::Ident {
                    name: "ID".into(),
                    quoted: true
                }
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn ellipsis_groups_into_when() {
    let m = Macro::parse(
        "M",
        "Begin\n    Condition =\"[A]=1\"\n    Action =\"Beep\"\nEnd\nBegin\n    Condition =\"...\"\n    Action =\"StopMacro\"\nEnd\nBegin\n    Action =\"Beep\"\nEnd\n"
            .into(),
    )
    .unwrap();
    match &m.entry().steps[..] {
        [Step::When { cond, body }, Step::Always(Action::Beep)] => {
            assert_eq!(cond.to_sqlite(), r#""A" = 1"#);
            assert!(matches!(&body[..], [Action::Beep, Action::StopMacro]));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn unknown_action_keeps_arguments() {
    let m = Macro::parse(
        "M",
        "Begin\n    Action =\"RunCode\"\n    Argument =\"Startup()\"\nEnd\n".into(),
    )
    .unwrap();
    match &m.entry().steps[..] {
        [Step::Always(Action::Unknown { name, arguments })] => {
            assert_eq!(name, "RunCode");
            assert_eq!(arguments, &["Startup()".to_string()]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn filters_submacro_names_and_set_last_filter_id() {
    let Some(pkg) = projects() else { return };
    let m = pkg.ui_macro("Filters").unwrap();
    let names: Vec<Option<&str>> = m.submacros.iter().map(|s| s.name.as_deref()).collect();
    assert_eq!(
        names,
        vec![
            None,
            Some("ApplyFilterFavorite"),
            Some("New"),
            Some("SetLastFilterID"),
            Some("Manage"),
            Some("SetupTempVars"),
            Some("RemoveTempVars"),
            Some("ClearFilter"),
            Some("CheckFilter"),
        ]
    );
    let set = m.submacro("SetLastFilterID").unwrap();
    match &set.steps[..] {
        [Step::Always(Action::SetTempVar { name, .. })] => {
            assert_eq!(name, "LastFilterCreated");
        }
        other => panic!("expected first step SetTempVar, got {other:?}"),
    }
}

#[test]
fn filters_apply_filter_favorite_ellipsis() {
    let Some(pkg) = projects() else { return };
    let m = pkg.ui_macro("Filters").unwrap();
    let fav = m.submacro("ApplyFilterFavorite").unwrap();
    match &fav.steps[0] {
        Step::When { cond, body } => {
            assert_eq!(
                cond.to_sqlite(),
                "(:screen_ActiveControl IS NULL) OR :screen_ActiveControl = 0"
            );
            match &body[..] {
                [Action::RunMacro { name }, Action::StopMacro] => {
                    assert_eq!(name, "Filters.ClearFilter");
                }
                other => panic!("{other:?}"),
            }
        }
        _ => unreachable!(),
    }
    match &fav.steps[1] {
        Step::When { cond, body } => {
            assert_eq!(cond.to_sqlite(), ":screen_ActiveControl = -1");
            match &body[..] {
                [Action::RunMacro { name }, Action::StopMacro] => {
                    assert_eq!(name, "Filters.Manage");
                }
                other => panic!("{other:?}"),
            }
        }
        _ => unreachable!(),
    }
    let apply = fav.steps.iter().find_map(|s| {
        let actions: &[Action] = match s {
            Step::Always(a) => std::slice::from_ref(a),
            Step::When { body, .. } => body,
        };
        actions.iter().find_map(|a| match a {
            Action::ApplyFilter {
                where_condition, ..
            } => where_condition.as_ref(),
            _ => None,
        })
    });
    let where_e = apply.expect("ApplyFilter");
    assert_eq!(where_e.to_sqlite(), ":tempvar_FilterString");
}

#[test]
fn filters_checkfilter_new_manage_clear() {
    let Some(pkg) = projects() else { return };
    let m = pkg.ui_macro("Filters").unwrap();
    let check = m.submacro("CheckFilter").unwrap();
    match &check.steps[..] {
        [Step::When { cond, body }] => {
            assert_eq!(
                *cond,
                parse_expr("Not ([Form].[FilterOn] Or [Form].[OrderByOn])").unwrap()
            );
            match &body[..] {
                [Action::MsgBox { .. }, Action::StopMacro] => {}
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
    let new = m.submacro("New").unwrap();
    let open = new.steps.iter().find_map(|s| match s {
        Step::Always(Action::OpenForm {
            form,
            view,
            where_condition,
            data_mode,
            window_mode,
            ..
        }) => Some((form, view, where_condition, data_mode, window_mode)),
        _ => None,
    });
    let (form, view, where_c, data, window) = open.expect("New OpenForm");
    assert_eq!(form, "Filter Details");
    assert_eq!(*view, FormView::Normal);
    assert!(where_c.is_none());
    assert_eq!(*data, FormDataMode::Add);
    assert_eq!(*window, WindowMode::Dialog);

    let manage = m.submacro("Manage").unwrap();
    let open = manage.steps.iter().find_map(|s| match s {
        Step::Always(Action::OpenForm {
            data_mode,
            where_condition,
            window_mode,
            ..
        }) => Some((data_mode, where_condition, window_mode)),
        _ => None,
    });
    let (data, where_c, window) = open.expect("Manage OpenForm");
    assert_eq!(*data, FormDataMode::PropertySettings);
    assert_eq!(*window, WindowMode::Dialog);
    let w = where_c.as_ref().expect("where");
    assert_eq!(
        *w,
        parse_expr("[Object Name]=[Application].[CurrentObjectName]").unwrap()
    );
    assert!(
        manage
            .steps
            .iter()
            .any(|s| matches!(s, Step::Always(Action::RunCommand(AcCmd::Refresh)))),
        "{:?}",
        manage.steps
    );
    let clear = m.submacro("ClearFilter").unwrap();
    assert!(
        clear.steps.iter().any(|s| matches!(
            s,
            Step::When { body, .. } if body.iter().any(|a| matches!(a, Action::RunCommand(AcCmd::RemoveAllFilters)))
        )),
        "{:?}",
        clear.steps
    );
}

#[test]
fn embedded_openform_project_details_where_is_concat() {
    let Some(pkg) = projects() else { return };
    let mut found = None;
    for d in pkg
        .forms()
        .unwrap()
        .into_iter()
        .chain(pkg.reports().unwrap())
    {
        for m in d.embedded_macros().unwrap().into_values() {
            for sm in &m.submacros {
                for step in &sm.steps {
                    let actions: &[Action] = match step {
                        Step::Always(a) => std::slice::from_ref(a),
                        Step::When { body, .. } => body,
                    };
                    for a in actions {
                        if let Action::OpenForm {
                            form,
                            where_condition: Some(w),
                            window_mode,
                            ..
                        } = a
                            && form == "Project Details"
                            && *window_mode == WindowMode::Dialog
                        {
                            let expect = parse_expr(r#""[ID]=" & Nz([ID],0)"#).unwrap();
                            if *w == expect {
                                found = Some(w.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    let w = found.expect("OpenForm Project Details with concat where");
    let expect = parse_expr(r#""[ID]=" & Nz([ID],0)"#).unwrap();
    assert_eq!(w, expect);
}

#[test]
fn sendobject_on_task_details() {
    let Some(pkg) = projects() else { return };
    let f = pkg.form("Task Details").unwrap();
    let mut found = None;
    for m in f.embedded_macros().unwrap().into_values() {
        for sm in &m.submacros {
            for step in &sm.steps {
                let actions: &[Action] = match step {
                    Step::Always(a) => std::slice::from_ref(a),
                    Step::When { body, .. } => body,
                };
                for a in actions {
                    if let Action::SendObject {
                        object_type: ObjectType::None,
                        to: Some(to),
                        subject: Some(subj),
                        message: Some(msg),
                        ..
                    } = a
                    {
                        found = Some((to.clone(), subj.clone(), msg.clone()));
                    }
                }
            }
        }
    }
    let (to, subj, msg) = found.expect("SendObject None");
    assert!(
        matches!(to, Expr::Call { ref name, .. } if name.eq_ignore_ascii_case("DLookUp")),
        "{to:?}"
    );
    assert!(
        matches!(subj, Expr::Call { ref name, .. } if name.eq_ignore_ascii_case("Replace")),
        "{subj:?}"
    );
    assert!(
        matches!(msg, Expr::Call { ref name, .. } if name.eq_ignore_ascii_case("IIf")),
        "{msg:?}"
    );
}

#[test]
fn runcommand_histogram() {
    let Some(pkg) = projects() else { return };
    let mut counts = std::collections::BTreeMap::<u16, usize>::new();
    let mut bump = |cmd: AcCmd| {
        *counts.entry(cmd.code()).or_insert(0) += 1;
    };
    let walk_macro = |m: &Macro, bump: &mut dyn FnMut(AcCmd)| {
        for sm in &m.submacros {
            for step in &sm.steps {
                let actions: &[Action] = match step {
                    Step::Always(a) => std::slice::from_ref(a),
                    Step::When { body, .. } => body,
                };
                for a in actions {
                    if let Action::RunCommand(c) = a {
                        bump(*c);
                    }
                }
            }
        }
    };
    for m in pkg.macros().unwrap() {
        walk_macro(&m, &mut bump);
    }
    for d in pkg
        .forms()
        .unwrap()
        .into_iter()
        .chain(pkg.reports().unwrap())
    {
        for m in d.embedded_macros().unwrap().into_values() {
            walk_macro(&m, &mut bump);
        }
    }
    assert_eq!(counts.get(&97).copied().unwrap_or(0), 23, "{counts:?}");
    assert!(counts.contains_key(&18), "Refresh: {counts:?}");
    assert!(counts.contains_key(&144), "RemoveFilterSort: {counts:?}");
    assert!(counts.contains_key(&292), "Undo: {counts:?}");
    assert!(counts.contains_key(&583), "AddFromOutlook: {counts:?}");
    assert!(
        counts.contains_key(&584),
        "SaveAsOutlookContact: {counts:?}"
    );
    assert!(counts.contains_key(&644), "RemoveAllFilters: {counts:?}");
    assert_eq!(AcCmd::from(97), AcCmd::SaveRecord);
    assert_eq!(AcCmd::from(18), AcCmd::Refresh);
    assert_eq!(AcCmd::from(223), AcCmd::DeleteRecord);
    assert_eq!(AcCmd::from(340), AcCmd::Print);
    assert_eq!(AcCmd::from(608), AcCmd::Unknown(608));
    assert!(counts.contains_key(&223), "{counts:?}");
    assert!(counts.contains_key(&340), "{counts:?}");
    assert!(counts.contains_key(&608), "{counts:?}");
}

#[test]
fn northwind_autoexec_unnamed_only() {
    let Some(pkg) = open_fixture("northwind-2.0-dev.accdt") else {
        return;
    };
    let m = pkg.ui_macro("AutoExec").unwrap();
    assert!(
        m.submacros.iter().all(|s| s.name.is_none()),
        "{:?}",
        m.submacros
    );
    assert_eq!(m.submacros.len(), 1);
    let steps: Vec<&Step> = m
        .entry()
        .steps
        .iter()
        .filter(|s| !is_axl_comment(s))
        .collect();
    assert_eq!(steps.len(), 2, "{steps:?}");
    match steps[0] {
        Step::When { cond, body } => {
            assert_eq!(
                *cond,
                parse_expr("Not [CurrentProject].[IsTrusted]").unwrap()
            );
            assert!(
                matches!(body.first(), Some(Action::OpenForm { form, .. }) if form == "frmStartup")
            );
        }
        other => panic!("{other:?}"),
    }
    match steps[1] {
        Step::When { cond, body } => {
            assert_eq!(*cond, parse_expr("[CurrentProject].[IsTrusted]").unwrap());
            assert!(matches!(
                body.first(),
                Some(Action::Unknown { name, .. }) if name == "RunCode"
            ));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn sync_combo_autoexec() {
    let Some(pkg) = open_fixture("sync-combo.accdt") else {
        return;
    };
    let m = pkg.ui_macro("AutoExec").unwrap();
    assert_eq!(m.submacros.len(), 1);
    assert!(m.submacros[0].name.is_none());
    let steps: Vec<&Step> = m
        .entry()
        .steps
        .iter()
        .filter(|s| !is_axl_comment(s))
        .collect();
    assert_eq!(steps.len(), 2, "{steps:?}");
}

#[test]
fn onerror_is_decoded() {
    let m = Macro::parse(
        "M",
        "Begin\n    Action =\"OnError\"\n    Argument =\"0\"\nEnd\nBegin\n    Action =\"OnError\"\n    Argument =\"2\"\nEnd\n"
            .into(),
    )
    .unwrap();
    match &m.entry().steps[..] {
        [
            Step::Always(Action::OnError {
                next: ErrorNext::Next,
                ..
            }),
            Step::Always(Action::OnError {
                next: ErrorNext::Fail,
                ..
            }),
        ] => {}
        other => panic!("{other:?}"),
    }
}
