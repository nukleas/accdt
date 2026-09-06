# Changelog

## 0.2.0

A breaking release. Everything a consumer previously had to re-derive from raw property
strings is now typed, and the two languages inside a template (Access expressions and
macros) are parsed rather than handed over as text.

### Added

- **Expressions** (`accdt::expr`). A parser for the Access expression language as it appears
  in query slots, `ControlSource`, `DefaultValue`, macro conditions and saved filter strings:
  bracketed identifiers, `"strings"`, `#dates#`, `&`, bang paths, `Like`, `Between`, `In`.
  `Expr::to_sqlite()` renders SQLite (`IIf`/`Nz`/`IsNull`/`CCur` lowered, `Like` wildcards
  translated, `Forms!`/`TempVars!` references as `:parameters`), `Display` renders Access
  syntax back, and `parameters()` lists what a caller must bind. `Query::to_sqlite()`
  translates every expression slot of a reconstructed query.
- **Macros as a tree.** `Macro` is now submacros of steps: a `MacroName` starts a `Submacro`,
  a run of `...` conditions becomes one `Step::When`, and each step is a typed `Action` with
  decoded arguments (`OpenForm` with view, data and window modes, `RunCommand(AcCmd)` over
  the full Access constant table, `SendObject`, `SetTempVar`, …). Expression-bearing
  arguments are `Expr`. AXL `If`/`ElseIf` map onto the same `Step::When`.
- **Typed design views.** `Design::controls()`/`sections()` return borrowed `Control` and
  `Section` views over the parsed document: `ControlKind`, `DefaultView`, `TextAlign`,
  `is_visible()`/`is_enabled()`/`is_locked()`/`column_hidden()` decoding Access's
  `NotDefault` convention, `format()`, `decimal_places()`, `layout()`, `is_attached_label()`.
- **Lookups, links and grouping.** `Control::lookup()` and `Table::column_lookup()` classify a
  row source (value list, SQL, named, field list, callback) with column count, widths and
  bound column. `Control::subform_link()` gives the target and the child/master field pairs.
  `Design::group_levels()` reads a report's `BreakLevel` blocks, associated with their header
  and footer sections.
- **Typed sample-data cells.** A cell is `Cell::{Null, Value, Invalid}`; `Null` distinguishes
  an absent element from `xsi:nil`, `Value` keeps the XML lexical text beside the decoded
  `Value::{Boolean, Integer, Double, Currency, Decimal, DateTime, Binary, Guid, Complex, …}`,
  and `Invalid` carries the part, table, row and column of what did not decode.
  `Table::validate_values()` collects those. Attachment and multi-valued children have their
  own schemas.
- **Descriptions.** `DesignDescription` and `TableDescription`: owned, serialisable summaries
  with a text `Display`, printed by the new `accdt describe <form|report|table>` command.
- **Package resolution.** `resolve_name()`, `resolve_record_source()` and
  `resolve_embedded_source()` resolve a saved name against the package's own object index.
- Non-select SQL reconstruction: append (`INSERT INTO … SELECT`), update, delete and
  make-table queries.

### Changed

- `Design::controls()` no longer returns owned `Control` structs with public fields. Use the
  accessors (`name()`, `kind()`, `section()`, `raw_property()`); sections are their own type
  rather than entries in the control list.
- `Macro::parse` and `Macro::from_node` are fallible, and so is `Design::embedded_macros()`.
  `MacroAction` is gone; read `submacros`, or `entry()` for the first one.
- `Table::rows` is `Vec<Row>` of typed `Cell`s rather than `Vec<Vec<Option<Value>>>`.
- `Table::sharepoint` is `sharepoint_metadata`, and `SharePointList` is `SharePointMetadata`:
  these properties appear on local tables too. `is_linked()` is now true only when the table
  has no data part, so a desktop template's local tables are no longer reported as links.
- `Design::record_source()` returns a classified `RecordSource` instead of `Option<&str>`.
- `accdt info` distinguishes a local table that carries a list template id from a linked list.

### Fixed

- A translated `UPDATE` uses bare column names in `SET`, which SQLite requires.
- Integration tests only require the fixture that `scripts/fetch-fixtures.sh` can download;
  the ones that come off the Access 2010 install media skip when absent.

## 0.1.1

- Reconstruct append, update, delete and make-table SQL from a query definition.
- A table with `WSS*` properties and a data part is local data, not a SharePoint link.

## 0.1.0

First release: the package (tables with schema and sample data, forms, reports, macros,
queries, modules, relationships, VBA references, resources, web-database AXL objects,
SharePoint list definitions and localisation variations) and the `accdt` CLI.
