use accdt::*;

fn form(body: &str) -> Design {
    Design::parse(
        "test",
        DesignKind::Form,
        format!(
            "Begin Form\nDefaultView =5\nBegin\nBegin TextBox\nWidth =100\nEnd\nBegin Section\nName =\"Detail\"\nBegin\n{body}\nEnd\nEnd\nEnd\nEnd\n"
        ),
    )
}

#[test]
fn borrowed_kinds_defaults_and_labels() {
    let d = form(
        "Begin TextBox\nName =\"field\"\nVisible =NotDefault\nLocked =NotDefault\nDecimalPlaces =0\nFormat =\"Short Date\"\nBegin\nBegin Label\nName =\"field_LayoutLabel\"\nEnd\nEnd\nEnd",
    );
    assert_eq!(d.controls().len(), 2);
    assert_eq!(d.sections().len(), 1);
    assert_eq!(d.items().len(), 3);
    assert_eq!(d.default_view().unwrap().unwrap().value, DefaultView::Split);
    let c = d.controls()[0];
    assert_eq!(c.kind(), ControlKind::TextBox);
    assert!(!c.is_visible().unwrap().unwrap().value);
    assert!(c.is_enabled().unwrap().unwrap().value);
    assert!(c.is_locked().unwrap().unwrap().value);
    assert!(!c.column_hidden().unwrap().unwrap().value);
    assert_eq!(c.layout().unwrap().width, Some(100));
    assert_eq!(
        c.decimal_places().unwrap().unwrap().value,
        DecimalPlaces::Fixed(0)
    );
    assert_eq!(
        c.format(),
        Some(DisplayFormat::Named(NamedFormat::ShortDate))
    );
    let label = d.controls()[1];
    assert!(label.is_attached_label());
    assert!(label.is_layout_label_candidate());
    assert_eq!(label.parent().unwrap().name(), "field");
    assert_eq!(label.section().unwrap().name(), "Detail");
}

#[test]
fn invalid_unknown_and_custom_defaults_are_explicit() {
    let d = form("Begin TextBox\nName =\"field\"\nVisible =maybe\nTextAlign =99\nWidth =oops\nEnd");
    let c = d.controls()[0];
    assert!(c.is_visible().is_err());
    assert!(c.layout().is_err());
    assert_eq!(
        c.text_align().unwrap().unwrap().value,
        TextAlign::Unknown(99)
    );
    let d = Design::parse("custom", DesignKind::Form, "Begin Form\nBegin\nBegin TextBox\nVisible =NotDefault\nEnd\nBegin TextBox\nName =\"x\"\nVisible =NotDefault\nEnd\nEnd\nEnd".into());
    assert!(
        d.controls()[0]
            .is_visible()
            .unwrap_err()
            .reason
            .contains("paired export")
    );
    assert_eq!(ControlKind::from_name("Textbox"), ControlKind::TextBox);
    let report = Design::parse("r", DesignKind::Report, "Begin Report\nBegin FormHeader\nName =\"Title\"\nEnd\nBegin BreakHeader\nName =\"GroupHeader3\"\nEnd\nEnd".into());
    assert_eq!(report.sections()[0].kind(), SectionKind::ReportHeader);
    assert_eq!(report.sections()[1].kind(), SectionKind::GroupHeader);
}

#[test]
fn lookup_tokens_columns_and_errors() {
    let d = form(
        r#"Begin ComboBox
Name ="values"
RowSourceType ="Value List"
RowSource ="\"code\";\"caption\";\"001\";\" a;b \";\"002\";\"say \"\"hi\"\"\""
ColumnCount =2
ColumnHeads =NotDefault
ColumnWidths ="0"
End"#,
    );
    let l = d.controls()[0].lookup().unwrap().unwrap();
    assert_eq!(l.bound, BoundColumn::Column(0));
    assert_eq!(l.widths, vec![ColumnWidth::Twips(0), ColumnWidth::Auto]);
    assert_eq!(l.display_column(), Some(1));
    let RowSource::Values(v) = l.source else {
        panic!("value list")
    };
    assert_eq!(v.headers, Some(vec!["code".into(), "caption".into()]));
    assert_eq!(
        v.rows,
        vec![vec!["001", " a;b "], vec!["002", "say \"hi\""]]
    );
    for source in ["a;b;c", "\\\"unclosed"] {
        let d = form(&format!(
            "Begin ComboBox\nName =\"bad\"\nColumnCount =2\nRowSourceType =\"Value List\"\nRowSource =\"{source}\"\nEnd"
        ));
        assert!(d.controls()[0].lookup().is_err());
    }
    let d = form(
        "Begin ComboBox\nName =\"empty\"\nRowSourceType =\"Value List\"\nRowSource =\"a;\"\nBoundColumn =0\nColumnWidths =\"0\"\nEnd",
    );
    let l = d.controls()[0].lookup().unwrap().unwrap();
    assert_eq!(l.bound, BoundColumn::RowIndex);
    assert_eq!(l.display_column(), None);
    assert_eq!(
        l.source,
        RowSource::Values(ValueList {
            rows: vec![vec!["a".into()], vec!["".into()]],
            headers: None
        })
    );
}

#[test]
fn groups_follow_structure_not_suffixes() {
    let d = Design::parse(
        "groups",
        DesignKind::Report,
        r#"Begin Report
Begin
Begin BreakLevel
ControlSource ="Outer"
GroupHeader =NotDefault
GroupFooter =NotDefault
End
Begin BreakLevel
ControlSource ="Inner"
GroupHeader =NotDefault
GroupFooter =NotDefault
KeepTogether =2
End
Begin BreakHeader
Name ="GroupHeader9"
End
Begin BreakHeader
Name ="Renamed"
End
Begin Section
Name ="Detail"
End
Begin BreakFooter
Name ="InnerFooter"
End
Begin BreakFooter
Name ="OuterFooter"
End
End
End"#
            .into(),
    );
    let groups = d.group_levels().unwrap();
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].header_section.unwrap().name(), "GroupHeader9");
    assert_eq!(groups[0].footer_section.unwrap().name(), "OuterFooter");
    assert_eq!(groups[1].footer_section.unwrap().name(), "InnerFooter");
    assert_eq!(groups[1].keep_together, GroupKeepTogether::WithFirstDetail);
    let broken = Design::parse(
        "bad",
        DesignKind::Report,
        d.text()
            .replace("GroupFooter =NotDefault", "GroupFooter =0"),
    );
    let levels = broken.group_levels().unwrap();
    assert!(levels[0].header_section.is_none());
    assert!(!levels[0].diagnostics.is_empty());
}

#[test]
fn subform_composite_links_preserve_names() {
    let d = form(
        "Begin Subform\nName =\"child\"\nSourceObject =\"Report.Tasks.Subreport\"\nLinkChildFields =\"[Project;ID];Owner\"\nLinkMasterFields =\"[ID];OwnerControl\"\nEnd",
    );
    let l = d.controls()[0].subform_link().unwrap().unwrap();
    assert_eq!(l.source, EmbeddedSource::Report("Tasks.Subreport"));
    assert_eq!(
        l.fields,
        vec![
            LinkField {
                child: "[Project;ID]",
                master: "[ID]"
            },
            LinkField {
                child: "Owner",
                master: "OwnerControl"
            }
        ]
    );
    let d = form("Begin Subform\nName =\"bad\"\nLinkChildFields =\"ID\"\nEnd");
    assert!(d.controls()[0].subform_link().is_err());
    let d = form("Begin Subform\nName =\"unlinked\"\nEnd");
    assert_eq!(
        d.controls()[0].subform_link().unwrap().unwrap().source,
        EmbeddedSource::Empty
    );
}

#[test]
fn projects_group_and_source_evidence() {
    let pkg = Package::open("tests/fixtures/project-management.accdt")
        .expect("Projects fixture is required for design integration tests");
    let reports = pkg.reports().unwrap();
    assert_eq!(
        reports
            .iter()
            .map(|d| d.group_levels().unwrap().len())
            .sum::<usize>(),
        16
    );
    for report in &reports {
        for g in report.group_levels().unwrap() {
            assert!(
                g.diagnostics.is_empty(),
                "{}: {:?}",
                report.name(),
                g.diagnostics
            );
        }
    }
    let d = pkg.report("Employee Address Book").unwrap();
    let groups = d.group_levels().unwrap();
    assert_eq!(groups[0].control_source, "File As");
    assert_eq!(groups[0].header_section.unwrap().name(), "GroupHeader3");
    assert_eq!(groups[0].group_on, GroupOn::PrefixCharacters);
    assert_eq!(groups[0].keep_together, GroupKeepTogether::WithFirstDetail);
    let d = pkg.form("Project Details").unwrap();
    assert!(
        matches!(pkg.resolve_record_source(d.record_source()), ResolvedRecordSource::Table(o) if o.name == "Projects")
    );
    let d = pkg.form("Project List").unwrap();
    assert!(
        matches!(pkg.resolve_record_source(d.record_source()), ResolvedRecordSource::Query(o) if o.name == "Open Projects")
    );
    let owner = d
        .controls()
        .into_iter()
        .find(|c| c.name() == "Owner")
        .unwrap();
    let lookup = owner.lookup().unwrap().unwrap();
    assert!(matches!(lookup.source, RowSource::Sql(_)));
    assert_eq!(
        lookup.widths,
        vec![ColumnWidth::Twips(0), ColumnWidth::Twips(2880)]
    );
    assert_eq!(lookup.bound, BoundColumn::Column(0));
    assert!(matches!(
        pkg.resolve_record_source(RecordSource::Named("[pRoJeCtS]")),
        ResolvedRecordSource::Table(_)
    ));
    assert!(matches!(
        pkg.resolve_record_source(RecordSource::Named("missing")),
        ResolvedRecordSource::Missing("missing")
    ));
    let d = pkg.form("Task Details").unwrap();
    assert!(matches!(
        pkg.resolve_record_source(d.record_source()),
        ResolvedRecordSource::Sql(_)
    ));
    let forms = pkg.forms().unwrap();
    let links: Vec<_> = forms
        .iter()
        .chain(reports.iter())
        .flat_map(|d| d.controls())
        .filter_map(|c| c.subform_link().unwrap())
        .collect();
    assert_eq!(links.len(), 5);
    assert_eq!(links.iter().filter(|l| l.fields.is_empty()).count(), 1);
    for l in links {
        assert!(
            matches!(
                pkg.resolve_embedded_source(l.source),
                ResolvedObject::Found(_)
            ),
            "{l:?}"
        );
    }
}
