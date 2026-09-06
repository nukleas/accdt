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
}

for f in pkg.forms()? {
    println!("{} on {:?}", f.name(), f.record_source());
    for c in f.controls() {
        println!("  {:?} {} in {:?}", c.kind(), c.name(), c.section().map(|s| s.kind()));
    }
    for (owner_event, m) in f.embedded_macros()? {
        println!("  macro {owner_event}: {} steps", m.entry().steps.len());
    }
}

for q in pkg.queries()? {
    println!("{}: {}", q.name, q.to_sql().text);        // Access SQL
    println!("{}: {}", q.name, q.to_sqlite()?.text);    // translated
}

// One page per object: sources, controls, lookups, links, events, group levels.
let form = pkg.form("Home")?;
print!("{}", accdt::DesignDescription::new(&form, Some(&pkg), false));
# Ok::<(), accdt::Error>(())
```

What is covered:

| Part | API |
|---|---|
| `template/template.xml`, `docProps/core.xml` | `template()`, `core_properties()` |
| `databaseProperties.xml` | `database_properties()` (AccessVersion, StartUpForm, AppTitle, …) |
| Tables: `objects/table*.xsd` + `sampleData/*.xml` | `tables()`, `table(name)`: columns with `od:jetType`/`od:sqlSType`, required, autoincrement, max length, field properties; indexes; table properties; rows as typed cells (`Cell::{Null, Value, Invalid}` with the XML lexical value kept; `Value::{Text, Boolean, Integer, Double, Currency, Decimal, DateTime, Binary, Guid, Complex}`, attachment children with their own schema); `validate_values()`; `column_lookup(field)` for a field displayed as a combo or list box; `to_csv()` |
| `dataMacros/*.axl` | `data_macros(table)` |
| Forms and reports (SaveAsText) | `forms()`, `reports()`, `form(name)`, `report(name)`: borrowed typed views (`sections()`, `controls()` with `ControlKind`, `is_visible()`/`is_enabled()`/`is_locked()`/`column_hidden()` decoding `NotDefault`, `default_view()`, `format()`, `decimal_places()`, `layout()`), `is_attached_label()`, `lookup()` (value lists vs SQL vs named row sources, bound and display columns), `subform_link()`, `record_source()` classified and resolved against the package, `group_levels()` from the report's `BreakLevel` blocks, events, embedded macros as parsed `Macro`s, code-behind VBA |
| Macros (SaveAsText) | `macros()`, `ui_macro(name)`: submacros, `Step::When` for `...` condition groups, typed `Action` (OpenForm, RunCommand/`AcCmd`, …) |
| Queries (SaveAsText) | `queries()`, `query(name)`: tables and aliases, columns, joins, where/having/group/order, parameters, properties; `to_sql()` returns the stored SQL when Access kept it (union, pass-through, `TOP`), otherwise rebuilds select, append (`INSERT INTO … SELECT`), update, delete and make-table queries with alias-aware joins and a `PARAMETERS` clause, and says which it did; crosstab, DDL and pass-through queries without stored SQL come back as a commented skeleton. `to_sqlite()` translates every expression slot of a reconstructed query |
| Expressions (query slots, `ControlSource`, `DefaultValue`, macro conditions, saved filter strings) | `expr::parse_expr` and friends return an `Expr` tree; `to_sqlite()` renders it (`[brackets]` quoted, `IIf`/`Nz`/`IsNull`/`CCur` lowered, `&` to `\|\|`, `Like` wildcards, `#dates#`), `Display` renders it back in Access syntax, `parameters()` lists the `Forms!`/`TempVars!`/`[Parent]!` references it needs bound |
| Modules | `modules()`, `module(name)` |
| Descriptions (what a port reads first) | `DesignDescription::new(design, package, all_controls)` and `TableDescription::new(table, package)`: owned, serialisable, with a text `Display`; `accdt describe` prints them |
| `relationships.xml` | `relationships()` with integrity and cascade flags |
| `vbaReferences.xml` | `vba_references()` with names for well-known type libraries and a 32-bit-only flag |
| `resources/` | `resources()` |
| Web databases (Access 2010 Access Services): forms/reports/queries as AXL XML | Same `forms()`/`reports()`/`queries()` API; `Design::is_axl()`, `ObjectEntry::format`. AXL UI macros appear as embedded macros; report items and query definitions map onto the same types |
| SharePoint list links (`WSS*` table properties, metadata `WSSTemplateID`) | `Table::sharepoint_metadata` (`SharePointMetadata`: template id, root folder, view URL, version), `Table::is_linked()` |
| List definitions (`.caml` parts, `ListInstanceDefinition` relationships) | `list_definitions()`: list name, template id, fields with types |
| Localisation variations (`template/variation` relationships: `FlipName`, `AddFurigana`, …) | grouped under the base object as `ObjectEntry::variations`; `form_variation()`, `report_variation()`, `query_variation()`, `table_variation()` |
| Linked tables (`Link` / `SQLLink` object types) | indexed as `ObjectKind::Link` with the raw part; no Microsoft template ships one |
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
accdt describe projects.accdt form "Project Details"   # sources, sections, controls, lookups, links, events, groups
accdt describe projects.accdt table Tasks --json
accdt json northwind.accdt form frmLogin      # controls, events, macros, code
accdt export northwind.accdt out/
```

## Testing

Unit tests build packages in memory. `tests/northwind.rs` runs against Microsoft's
Northwind 2.0 Developer Edition template and requires it (set
`ACCDT_SKIP_FIXTURE_TESTS=1` to skip):

```bash
scripts/fetch-fixtures.sh   # downloads tests/fixtures/northwind-2.0-dev.accdt
cargo test
```

`tests/webdb.rs` covers the Access 2010 web-database and SharePoint list templates
(`wss-107-tasks.accdt`, `wss-1100-issues.accdt`, `access2010-contacts-web.accdt`). Those are
not on Microsoft's CDN; they are the `ACCESSTEMPLATE_*.ACCDT_1033` entries of `AccLR.cab`
on the Access 2010 install media (also `Templates\1033\Access\WSS\{107,1100}.accdt` on a
Windows Office install). Drop them into `tests/fixtures/` and the tests run; otherwise they
skip.

## Formats

[MS-ACCDT](https://learn.microsoft.com/en-us/openspecs/sharepoint_protocols/ms-accdt/0a4a68d7-7a85-4a27-ad74-730db57862d7)
describes the package, [MS-AXL](https://learn.microsoft.com/en-us/openspecs/sharepoint_protocols/ms-axl/77408f2c-8e18-46ef-892d-c835be9a1030)
the XML parts. `SaveAsText` has no public specification; the parser follows real exports
(property blocks, anonymous wrapper blocks, per-type default blocks, `CodeBehindForm`).

## License

MIT or Apache-2.0, at your option.
