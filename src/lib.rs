//! Read Microsoft Access template packages (`.accdt`) without Access.
//!
//! An `.accdt` is an OPC zip (the same container as `.docx`) in which Access writes every
//! object of a database as text: tables as XML Schema plus sample-data XML, forms, reports,
//! macros and queries in the `SaveAsText` format, VBA modules as source, plus database
//! properties, relationships and VBA references. Access produces one with
//! **File > Save As > Template**, which needs no macros or scripts, so a template is the
//! easiest complete export a non-technical user can make.
//!
//! ```no_run
//! let pkg = accdt::Package::open("northwind.accdt")?;
//! for table in pkg.tables()? {
//!     println!("{} ({} rows)", table.name, table.rows.len());
//! }
//! for form in pkg.forms()? {
//!     println!("{}: {} controls", form.name(), form.controls().len());
//! }
//! # Ok::<(), accdt::Error>(())
//! ```
//!
//! Web databases (Access 2010 Access Services templates) store forms, reports and queries
//! as AXL XML instead of SaveAsText; they are read into the same types, and their SharePoint
//! list links and list definitions are exposed.
//!
//! Formats: [MS-ACCDT](https://learn.microsoft.com/en-us/openspecs/sharepoint_protocols/ms-accdt/)
//! for the package and [MS-AXL](https://learn.microsoft.com/en-us/openspecs/sharepoint_protocols/ms-axl/)
//! for the XML parts. The SaveAsText format is undocumented; the parser here is derived from
//! real exports.

mod axl;
mod database;
mod datamacro;
mod design;
mod error;
mod macros;
mod package;
mod query;
pub mod saveastext;
mod table;
pub mod text;

pub use axl::{ListDefinition, ListField};
pub use database::{CoreProperties, Property, Relationship, TemplateInfo, VbaReference};
pub use datamacro::{DataMacro, DataMacroAction};
pub use design::{Control, Design, DesignKind, Event, Layout};
pub use error::{Error, Result};
pub use macros::{Macro, MacroAction};
pub use package::{Module, ObjectEntry, ObjectKind, Package, PartFormat, Resource, Variation};
pub use query::{Join, Operation, OutputColumn, Query, QueryDef, Sql, SqlSource};
pub use table::{Column, Index, JetType, SharePointList, Table, Value};

pub use design::{
    BackStyle, ControlKind, DecimalPlaces, DefaultView, DesignItem, DisplayFormat, NamedFormat,
    PropertyError, PropertyOrigin, PropertyResult, Resolved, Section, SectionKind, TextAlign,
};

pub use design::NodeId;

pub use design::{
    BoundColumn, ColumnWidth, EmbeddedSource, GroupKeepTogether, GroupLevel, GroupOn, LinkField,
    Lookup, RecordSource, RowSource, SortDirection, SubformLink, ValueList,
};
pub use package::{ResolvedObject, ResolvedRecordSource};

pub use table::{
    Cell, ComplexRecord, Currency, Decimal, ExpandedName, NullEncoding, Restriction, Row,
    ValueError, XmlDateTime,
};
