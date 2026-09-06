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
