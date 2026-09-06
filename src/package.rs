//! The OPC container and the object index.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Seek};
use std::path::Path;

use crate::database::{self, CoreProperties, Property, Relationship, TemplateInfo, VbaReference};
use crate::datamacro::{self, DataMacro};
use crate::design::{Design, DesignKind};
use crate::macros::Macro;
use crate::query::Query;
use crate::table::{self, Table};
use crate::text;
use crate::{Error, Result};

const OBJECTS: &str = "template/database/objects/";
const REL_METADATA: &str = "template/object-metadata";
const REL_PROPERTIES: &str = "relationships/ObjectProperties";
const REL_TABLE_DATA: &str = "template/table-data";
const REL_DATAMACROS: &str = "relationships/DataMacros";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum ObjectKind {
    Table,
    Query,
    Form,
    Report,
    Macro,
    Module,
}

impl std::str::FromStr for ObjectKind {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<ObjectKind, String> {
        ObjectKind::from_type(s).ok_or_else(|| format!("unknown object kind {s:?}; expected table, query, form, report, macro or module"))
    }
}

impl ObjectKind {
    /// Parse `Table`, `form`, `MACRO`, … (case-insensitive).
    pub fn from_type(t: &str) -> Option<ObjectKind> {
        match t.to_ascii_lowercase().as_str() {
            "table" => Some(ObjectKind::Table),
            "query" => Some(ObjectKind::Query),
            "form" => Some(ObjectKind::Form),
            "report" => Some(ObjectKind::Report),
            "macro" => Some(ObjectKind::Macro),
            "module" => Some(ObjectKind::Module),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKind::Table => "table",
            ObjectKind::Query => "query",
            ObjectKind::Form => "form",
            ObjectKind::Report => "report",
            ObjectKind::Macro => "macro",
            ObjectKind::Module => "module",
        }
    }
}

/// One database object and the parts that describe it.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ObjectEntry {
    pub kind: ObjectKind,
    pub name: String,
    /// The main part (`.xsd` for tables, `.txt` for everything else).
    pub part: String,
    pub metadata_part: Option<String>,
    pub properties_part: Option<String>,
    pub data_part: Option<String>,
    pub datamacros_part: Option<String>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Module {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Resource {
    pub name: String,
    pub part: String,
    pub bytes: Vec<u8>,
}

/// An opened template package.
#[derive(Debug)]
pub struct Package {
    parts: BTreeMap<String, Vec<u8>>,
    objects: Vec<ObjectEntry>,
}

impl Package {
    /// Open an `.accdt` file, or a directory holding an unpacked one.
    pub fn open(path: impl AsRef<Path>) -> Result<Package> {
        let path = path.as_ref();
        if path.is_dir() {
            let mut parts = BTreeMap::new();
            walk_dir(path, path, &mut parts)?;
            Package::from_parts(parts, &path.display().to_string())
        } else {
            let file = std::fs::File::open(path)?;
            Package::from_reader(file).map_err(|e| match e {
                Error::Zip(zip::result::ZipError::InvalidArchive(_)) => Error::NotAPackage(path.display().to_string()),
                other => other,
            })
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Package> {
        Package::from_reader(Cursor::new(bytes))
    }

    pub fn from_reader<R: Read + Seek>(reader: R) -> Result<Package> {
        let mut zip = zip::ZipArchive::new(reader)?;
        let mut parts = BTreeMap::new();
        for i in 0..zip.len() {
            let mut f = zip.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            let mut buf = Vec::with_capacity(f.size() as usize);
            f.read_to_end(&mut buf)?;
            parts.insert(f.name().to_string(), buf);
        }
        Package::from_parts(parts, "<reader>")
    }

    fn from_parts(parts: BTreeMap<String, Vec<u8>>, origin: &str) -> Result<Package> {
        if !parts.keys().any(|k| k.starts_with("template/")) {
            return Err(Error::NotAPackage(origin.to_string()));
        }
        let mut pkg = Package { parts, objects: Vec::new() };
        pkg.objects = pkg.index_objects()?;
        Ok(pkg)
    }

    fn index_objects(&self) -> Result<Vec<ObjectEntry>> {
        let mut out = Vec::new();
        for name in self.parts.keys() {
            let Some(file) = name.strip_prefix(OBJECTS) else { continue };
            if file.contains('/') {
                continue;
            }
            let Some((base, ext)) = file.rsplit_once('.') else { continue };
            let rels = self.relationships_of(&format!("{OBJECTS}_rels/{file}.rels"))?;
            let find = |suffix: &str| rels.iter().find(|(t, _)| t.ends_with(suffix)).map(|(_, target)| resolve_target(OBJECTS, target));
            let metadata_part = find(REL_METADATA).or_else(|| {
                let p = format!("{OBJECTS}properties/{base}_Metadata.xml");
                self.parts.contains_key(&p).then_some(p)
            });
            let metadata = match &metadata_part {
                Some(p) => parse_metadata(p, self.part_required(p)?)?,
                None => None,
            };
            let (kind, obj_name) = match metadata {
                Some((t, n)) => match ObjectKind::from_type(&t) {
                    Some(k) => (k, n),
                    None => continue,
                },
                None => {
                    // No metadata part (hand-made packages): the file name carries the kind.
                    let prefixes = ["table", "query", "form", "report", "macro", "module"];
                    match prefixes.iter().find(|p| base.starts_with(*p)) {
                        Some(p) => (ObjectKind::from_type(p).unwrap(), base[p.len()..].to_string()),
                        None => continue,
                    }
                }
            };
            if (kind == ObjectKind::Table) != (ext == "xsd") {
                continue;
            }
            out.push(ObjectEntry {
                kind,
                name: obj_name,
                part: name.clone(),
                metadata_part,
                properties_part: find(REL_PROPERTIES),
                data_part: find(REL_TABLE_DATA),
                datamacros_part: find(REL_DATAMACROS),
            });
        }
        out.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        Ok(out)
    }

    /// `(relationship type, target)` pairs of an OPC `.rels` part; empty when absent, an
    /// error when present but malformed.
    fn relationships_of(&self, rels_part: &str) -> Result<Vec<(String, String)>> {
        let Some(bytes) = self.parts.get(rels_part) else { return Ok(Vec::new()) };
        let xml = text::decode_xml(bytes);
        let doc = text::parse_xml(rels_part, &xml)?;
        Ok(doc
            .descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == "Relationship")
            .filter_map(|n| Some((n.attribute("Type")?.to_string(), n.attribute("Target")?.to_string())))
            .collect())
    }

    /// Raw bytes of a part by its package path.
    pub fn part(&self, name: &str) -> Option<&[u8]> {
        self.parts.get(name).map(Vec::as_slice)
    }

    fn part_required(&self, name: &str) -> Result<&[u8]> {
        self.part(name).ok_or_else(|| Error::MissingPart(name.to_string()))
    }

    pub fn parts(&self) -> impl Iterator<Item = &str> {
        self.parts.keys().map(String::as_str)
    }

    /// Every database object with its parts, sorted by kind then name.
    pub fn objects(&self) -> &[ObjectEntry] {
        &self.objects
    }

    pub fn objects_of(&self, kind: ObjectKind) -> impl Iterator<Item = &ObjectEntry> {
        self.objects.iter().filter(move |o| o.kind == kind)
    }

    /// An object by kind and name (case-insensitive, as Access names are).
    pub fn object(&self, kind: ObjectKind, name: &str) -> Option<&ObjectEntry> {
        self.objects.iter().find(|o| o.kind == kind && o.name.eq_ignore_ascii_case(name))
    }

    fn object_required(&self, kind: ObjectKind, name: &str) -> Result<&ObjectEntry> {
        self.object(kind, name).ok_or_else(|| Error::MissingObject { kind: kind.as_str(), name: name.to_string() })
    }

    /// Template metadata; defaults when the part is absent.
    pub fn template(&self) -> Result<TemplateInfo> {
        let p = "template/template.xml";
        match self.part(p) {
            Some(b) => database::parse_template(p, b),
            None => Ok(TemplateInfo::default()),
        }
    }

    /// OPC core properties (title, description, …); defaults when the part is absent.
    pub fn core_properties(&self) -> Result<CoreProperties> {
        let p = "docProps/core.xml";
        match self.part(p) {
            Some(b) => database::parse_core(p, b),
            None => Ok(CoreProperties::default()),
        }
    }

    /// `databaseProperties.xml`: `AccessVersion`, `StartUpForm`, `AppTitle`, …; empty when absent.
    pub fn database_properties(&self) -> Result<Vec<Property>> {
        let p = "template/database/databaseProperties.xml";
        match self.part(p) {
            Some(b) => database::parse_properties(p, b),
            None => Ok(Vec::new()),
        }
    }

    pub fn relationships(&self) -> Result<Vec<Relationship>> {
        let p = "template/database/relationships.xml";
        match self.part(p) {
            Some(b) => database::parse_relationships(p, b),
            None => Ok(Vec::new()),
        }
    }

    pub fn vba_references(&self) -> Result<Vec<VbaReference>> {
        let p = "template/database/vbaReferences.xml";
        match self.part(p) {
            Some(b) => database::parse_references(p, b),
            None => Ok(Vec::new()),
        }
    }

    /// The object's `*_Properties.axl` properties (GUID, NameMap, PublishToWeb, …).
    pub fn object_properties(&self, object: &ObjectEntry) -> Result<Vec<Property>> {
        match &object.properties_part {
            Some(p) => database::parse_properties(p, self.part_required(p)?),
            None => Ok(Vec::new()),
        }
    }

    pub fn tables(&self) -> Result<Vec<Table>> {
        self.objects_of(ObjectKind::Table).map(|o| self.table_of(o)).collect()
    }

    /// A table by name; `Error::MissingObject` when there is none.
    pub fn table(&self, name: &str) -> Result<Table> {
        self.table_of(self.object_required(ObjectKind::Table, name)?)
    }

    fn table_of(&self, o: &ObjectEntry) -> Result<Table> {
        let mut t = table::parse_schema(&o.part, &o.name, self.part_required(&o.part)?)?;
        if let Some(dp) = &o.data_part {
            table::parse_data(dp, &mut t, self.part_required(dp)?)?;
        }
        Ok(t)
    }

    /// Data macros attached to a table (empty when the table has none).
    pub fn data_macros(&self, table: &str) -> Result<Vec<DataMacro>> {
        match &self.object_required(ObjectKind::Table, table)?.datamacros_part {
            Some(p) => datamacro::parse(p, self.part_required(p)?),
            None => Ok(Vec::new()),
        }
    }

    pub fn forms(&self) -> Result<Vec<Design>> {
        self.objects_of(ObjectKind::Form).map(|o| self.design_of(o, DesignKind::Form)).collect()
    }

    pub fn reports(&self) -> Result<Vec<Design>> {
        self.objects_of(ObjectKind::Report).map(|o| self.design_of(o, DesignKind::Report)).collect()
    }

    pub fn form(&self, name: &str) -> Result<Design> {
        self.design_of(self.object_required(ObjectKind::Form, name)?, DesignKind::Form)
    }

    pub fn report(&self, name: &str) -> Result<Design> {
        self.design_of(self.object_required(ObjectKind::Report, name)?, DesignKind::Report)
    }

    fn design_of(&self, o: &ObjectEntry, kind: DesignKind) -> Result<Design> {
        Ok(Design::parse(&o.name, kind, self.object_text(o)?))
    }

    pub fn modules(&self) -> Result<Vec<Module>> {
        self.objects_of(ObjectKind::Module).map(|o| self.module_of(o)).collect()
    }

    pub fn module(&self, name: &str) -> Result<Module> {
        self.module_of(self.object_required(ObjectKind::Module, name)?)
    }

    fn module_of(&self, o: &ObjectEntry) -> Result<Module> {
        Ok(Module { name: o.name.clone(), source: self.object_text(o)? })
    }

    pub fn macros(&self) -> Result<Vec<Macro>> {
        self.objects_of(ObjectKind::Macro).map(|o| Ok(Macro::parse(&o.name, self.object_text(o)?))).collect()
    }

    /// A standalone macro by name.
    pub fn ui_macro(&self, name: &str) -> Result<Macro> {
        let o = self.object_required(ObjectKind::Macro, name)?;
        Ok(Macro::parse(&o.name, self.object_text(o)?))
    }

    pub fn queries(&self) -> Result<Vec<Query>> {
        self.objects_of(ObjectKind::Query).map(|o| Ok(Query::parse(&o.name, self.object_text(o)?))).collect()
    }

    pub fn query(&self, name: &str) -> Result<Query> {
        let o = self.object_required(ObjectKind::Query, name)?;
        Ok(Query::parse(&o.name, self.object_text(o)?))
    }

    /// Raw text of any non-table object (SaveAsText or VBA).
    pub fn object_text(&self, object: &ObjectEntry) -> Result<String> {
        Ok(text::decode(self.part_required(&object.part)?))
    }

    /// Shared images and other resources (`template/database/resources/`), with their display names.
    pub fn resources(&self) -> Result<Vec<Resource>> {
        const DIR: &str = "template/database/resources/";
        let mut out = Vec::new();
        for (name, bytes) in &self.parts {
            let Some(file) = name.strip_prefix(DIR) else { continue };
            if file.contains('/') || file.ends_with("-name.txt") {
                continue;
            }
            let rels = self.relationships_of(&format!("{DIR}_rels/{file}.rels"))?;
            let display = rels
                .iter()
                .find(|(t, _)| t.ends_with("resource-name"))
                .and_then(|(_, target)| self.parts.get(&resolve_target(DIR, target)))
                .map(|b| text::decode(b).trim().to_string())
                .unwrap_or_else(|| file.to_string());
            out.push(Resource { name: display, part: name.clone(), bytes: bytes.clone() });
        }
        Ok(out)
    }
}

/// Resolve an OPC relationship target against the directory of the source part:
/// absolute targets (`/template/...`) are package-rooted, others are relative with `.`/`..` segments.
fn resolve_target(base_dir: &str, target: &str) -> String {
    let segments: Vec<&str> = match target.strip_prefix('/') {
        Some(abs) => abs.split('/').collect(),
        None => base_dir.trim_end_matches('/').split('/').chain(target.split('/')).collect(),
    };
    let mut out: Vec<&str> = Vec::with_capacity(segments.len());
    for seg in segments {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// `(Type, Name)` from an object's metadata part.
fn parse_metadata(part: &str, bytes: &[u8]) -> Result<Option<(String, String)>> {
    let xml = text::decode_xml(bytes);
    let doc = text::parse_xml(part, &xml)?;
    let root = doc.root_element();
    let get = |k: &str| root.children().find(|c| c.is_element() && c.tag_name().name() == k).and_then(|c| c.text()).map(|t| t.to_string());
    Ok(get("Type").zip(get("Name")))
}

fn walk_dir(dir: &Path, base: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let p = entry?.path();
        if p.is_dir() {
            walk_dir(&p, base, out)?;
        } else {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            out.insert(rel, std::fs::read(&p)?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve_target;

    #[test]
    fn resolves_relationship_targets() {
        let base = "template/database/objects/";
        assert_eq!(resolve_target(base, "sampleData/t.xml"), "template/database/objects/sampleData/t.xml");
        assert_eq!(resolve_target(base, "./sampleData/t.xml"), "template/database/objects/sampleData/t.xml");
        assert_eq!(resolve_target(base, "../relationships.xml"), "template/database/relationships.xml");
        assert_eq!(resolve_target(base, "/template/x.xml"), "template/x.xml");
    }
}
