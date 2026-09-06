# accdt

Read Microsoft Access template packages (`.accdt`) in pure Rust, without Access.

An `.accdt` is an OPC zip in which Access writes every object of a database as text:
tables as XML Schema plus data XML, forms, reports, macros and queries in the `SaveAsText`
format, VBA modules as source, plus database properties, relationships, VBA references and
shared images. Access creates one from **File > Save As > Template** ("Include data in
template" keeps the rows), with no macros or scripts involved, which makes it the simplest
complete export a non-technical user can produce. This crate turns that export back into
typed data.

```rust
let pkg = accdt::Package::open("northwind.accdt")?;

for t in pkg.tables()? {
    let pk = t.primary_key().map(|i| i.columns.join(", ")).unwrap_or_default();
    println!("{} ({} rows, pk {pk})", t.name, t.rows.len());
    for c in &t.columns {
        println!("  {} {:?} {}", c.name, c.jet_type, c.sql_type.as_deref().unwrap_or(""));
    }
}
for f in pkg.forms()? {
    println!("{}: source {:?}", f.name, f.record_source());
    for c in f.controls() {
        println!("  {} {} in {}", c.control_type, c.name, c.section);
    }
    for e in f.events() {
        println!("  {}.{} -> {}", e.owner, e.event, e.value);
    }
    for (owner_event, m) in f.embedded_macros() {
        println!("  macro {owner_event}: {} actions", m.actions.len());
    }
}
for q in pkg.queries()? {
    let sql = q.to_sql();
    println!("{}: {} ({:?}{})", q.name, sql.text, sql.source, if sql.complete { "" } else { ", partial" });
}
for r in pkg.vba_references()? {
    println!("{} {}", r.guid, r.known_name().unwrap_or("?"));
}
# Ok::<(), accdt::Error>(())
```

What is covered:

| Part | API |
|---|---|
| `template/template.xml`, `docProps/core.xml` | `template()`, `core_properties()` |
| `databaseProperties.xml` | `database_properties()` (AccessVersion, StartUpForm, AppTitle, …) |
| Tables: `objects/table*.xsd` + `sampleData/*.xml` | `tables()`, `table(name)`: columns with `od:jetType`/`od:sqlSType`, required, autoincrement, max length, field properties; indexes; table properties; rows (attachments and multi-valued cells as structured `Value::Complex` records); `to_csv()` |
| `dataMacros/*.axl` | `data_macros(table)` |
| Forms and reports (SaveAsText) | `forms()`, `reports()`, `form(name)`, `report(name)`: properties, control tree with layout, sections, events (`On*` and `AfterUpdate`-style), embedded macros as parsed `Macro`s, code-behind VBA, per-type control defaults |
| Macros (SaveAsText) | `macros()`, `ui_macro(name)`: actions with conditions and arguments |
| Queries (SaveAsText) | `queries()`, `query(name)`: tables and aliases, columns, joins, where/having/group/order, parameters, properties; `to_sql()` returns the stored SQL when Access kept it (union, pass-through, `TOP`), otherwise rebuilds select queries with alias-aware joins and a `PARAMETERS` clause, and says which it did |
| Modules | `modules()`, `module(name)` |
| `relationships.xml` | `relationships()` with integrity and cascade flags |
| `vbaReferences.xml` | `vba_references()` with names for well-known type libraries and a 32-bit-only flag |
| `resources/` | `resources()` |
| `*_Properties.axl`, `*_Metadata.xml` | `object_properties()`, `objects()` |
| Anything else | `part(name)` |

The `saveastext` module is public: it parses any `Application.SaveAsText` output, not only
templates. It joins wrapped string continuations, decodes `\"`, `\\` and `\015`-style
escapes, keeps `Key = Begin` blocks as hex values or nested blocks (embedded macros), and
reports structural problems in `Document::warnings` instead of failing.

Lookups are by name, case-insensitively, and return `Error::MissingObject` when absent.
Optional parts (`databaseProperties.xml`, `relationships.xml`, `vbaReferences.xml`,
`template.xml`, `docProps/core.xml`) read as empty or default when missing; malformed XML
anywhere is an `Error::Xml` naming the part.

## Command line

```bash
cargo install accdt --features cli
accdt info northwind.accdt
accdt ls northwind.accdt
accdt cat northwind.accdt table Companies      # CSV
accdt cat northwind.accdt query qryOrders     # SQL
accdt json northwind.accdt form frmLogin      # controls, events, macros, code
accdt export northwind.accdt out/
```

## Testing

Unit tests build packages in memory. The integration test in `tests/northwind.rs` runs
against Microsoft's Northwind 2.0 Developer Edition template when it is present:

```bash
scripts/fetch-fixtures.sh   # downloads tests/fixtures/northwind-2.0-dev.accdt
cargo test
```

## Formats

[MS-ACCDT](https://learn.microsoft.com/en-us/openspecs/sharepoint_protocols/ms-accdt/0a4a68d7-7a85-4a27-ad74-730db57862d7)
describes the package, [MS-AXL](https://learn.microsoft.com/en-us/openspecs/sharepoint_protocols/ms-axl/77408f2c-8e18-46ef-892d-c835be9a1030)
the XML parts. `SaveAsText` has no public specification; the parser follows real exports
(property blocks, anonymous wrapper blocks, per-type default blocks, `CodeBehindForm`).

## License

MIT or Apache-2.0, at your option.
