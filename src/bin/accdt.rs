//! `accdt` command line: inspect and export Access template packages.

use std::path::PathBuf;
use std::process::ExitCode;

use accdt::{ObjectKind, Package};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "accdt",
    version,
    about = "Read Microsoft Access template packages (.accdt) without Access"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Template metadata, properties, references and object counts
    Info { package: PathBuf },
    /// List objects (`kind<TAB>name`)
    Ls { package: PathBuf },
    /// Print one object: table as CSV, query as SQL, others as their text
    Cat {
        package: PathBuf,
        kind: String,
        name: String,
    },
    /// Print an object as JSON (design controls, query definition, table schema, …)
    Json {
        package: PathBuf,
        kind: String,
        name: String,
    },
    /// Write every object to a directory (csv, sql, txt, bas, json)
    Export { package: PathBuf, dir: PathBuf },
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("accdt: {e}");
            ExitCode::from(1)
        }
    }
}

fn kind(s: &str) -> Result<ObjectKind, Box<dyn std::error::Error>> {
    Ok(s.parse::<ObjectKind>()?)
}

/// File-system safe, collision-free names within one export: a second object that maps to
/// the same name gets a numeric suffix, and `names.json` records the mapping.
struct Namer {
    used: std::collections::HashMap<String, usize>,
    manifest: std::collections::BTreeMap<String, String>,
}

impl Namer {
    fn new() -> Namer {
        Namer {
            used: Default::default(),
            manifest: Default::default(),
        }
    }
    fn file(&mut self, dir: &str, object: &str) -> String {
        let base: String = object
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let key = format!("{dir}/{}", base.to_lowercase());
        let n = self.used.entry(key).or_insert(0);
        *n += 1;
        let name = if *n == 1 { base } else { format!("{base}~{n}") };
        self.manifest
            .insert(format!("{dir}/{name}"), object.to_string());
        name
    }
}

fn design_json(d: &accdt::Design) -> serde_json::Value {
    serde_json::json!({"name": d.name(), "properties": d.properties(), "controls": d.controls().iter().map(|c| c.raw_node()).collect::<Vec<_>>(), "events": d.events(), "embedded_macros": d.embedded_macros(), "code_behind": d.code_behind(), "warnings": d.document().warnings})
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Info { package } => {
            let pkg = Package::open(&package)?;
            if let Ok(c) = pkg.core_properties() {
                println!("title: {}", c.title.unwrap_or_default());
                if let Some(d) = c.description {
                    println!("description: {d}");
                }
            }
            if let Ok(t) = pkg.template() {
                println!(
                    "required access version: {}  type: {}",
                    t.required_access_version.unwrap_or_default(),
                    t.template_type.unwrap_or_default()
                );
            }
            for p in pkg.database_properties()? {
                if matches!(
                    p.name.as_str(),
                    "AccessVersion"
                        | "AppTitle"
                        | "StartUpForm"
                        | "StartUpShowDBWindow"
                        | "AllowBypassKey"
                ) {
                    println!("{}: {}", p.name, p.value);
                }
            }
            for k in [
                ObjectKind::Table,
                ObjectKind::Query,
                ObjectKind::Form,
                ObjectKind::Report,
                ObjectKind::Macro,
                ObjectKind::Module,
                ObjectKind::Link,
            ] {
                let n = pkg.objects_of(k).count();
                if n > 0 || k != ObjectKind::Link {
                    let axl = pkg
                        .objects_of(k)
                        .filter(|o| o.format == accdt::PartFormat::Axl)
                        .count();
                    let variations: usize = pkg.objects_of(k).map(|o| o.variations.len()).sum();
                    println!(
                        "{:<8} {}{}{}",
                        k.as_str(),
                        n,
                        if axl > 0 {
                            format!(" ({axl} axl)")
                        } else {
                            String::new()
                        },
                        if variations > 0 {
                            format!(" (+{variations} variations)")
                        } else {
                            String::new()
                        }
                    );
                }
            }
            for t in pkg.tables()? {
                if let Some(sp) = &t.sharepoint {
                    println!(
                        "sharepoint list: {} (template {})",
                        t.name,
                        sp.template_id.map(|i| i.to_string()).unwrap_or_default()
                    );
                }
            }
            for l in pkg.list_definitions()? {
                println!(
                    "list definition: {} (template {}, {} fields)",
                    l.list_name,
                    l.template_id.map(|i| i.to_string()).unwrap_or_default(),
                    l.fields.len()
                );
            }
            println!("relationships: {}", pkg.relationships()?.len());
            for r in pkg.vba_references()? {
                println!(
                    "reference: {} {}.{} {}{}",
                    r.guid,
                    r.major,
                    r.minor,
                    r.known_name().unwrap_or("?"),
                    if r.is_32bit_only() {
                        " [32-bit only]"
                    } else {
                        ""
                    }
                );
            }
        }
        Cmd::Ls { package } => {
            let pkg = Package::open(&package)?;
            for o in pkg.objects() {
                println!("{}\t{}", o.kind.as_str(), o.name);
            }
        }
        Cmd::Cat {
            package,
            kind: k,
            name,
        } => {
            let pkg = Package::open(&package)?;
            let k = kind(&k)?;
            match k {
                ObjectKind::Table => print!("{}", pkg.table(&name)?.to_csv()),
                ObjectKind::Query => {
                    let sql = pkg.query(&name)?.to_sql();
                    if !sql.is_executable() {
                        eprintln!(
                            "accdt: {} has no executable SQL ({:?}); printing the definition",
                            name, sql.source
                        );
                    }
                    println!("{}", sql.text);
                }
                _ => {
                    let o = pkg
                        .object(k, &name)
                        .ok_or_else(|| format!("no {} named {name}", k.as_str()))?;
                    print!("{}", pkg.object_text(o)?);
                }
            }
        }
        Cmd::Json {
            package,
            kind: k,
            name,
        } => {
            let pkg = Package::open(&package)?;
            let k = kind(&k)?;
            let json = match k {
                ObjectKind::Table => serde_json::to_string_pretty(&pkg.table(&name)?)?,
                ObjectKind::Query => {
                    let q = pkg.query(&name)?;
                    serde_json::to_string_pretty(
                        &serde_json::json!({"name": q.name, "definition": q.definition, "sql": q.to_sql()}),
                    )?
                }
                ObjectKind::Form => serde_json::to_string_pretty(&design_json(&pkg.form(&name)?))?,
                ObjectKind::Report => {
                    serde_json::to_string_pretty(&design_json(&pkg.report(&name)?))?
                }
                ObjectKind::Macro => serde_json::to_string_pretty(&pkg.ui_macro(&name)?)?,
                ObjectKind::Module => serde_json::to_string_pretty(&pkg.module(&name)?)?,
                ObjectKind::Link => {
                    let o = pkg
                        .object(k, &name)
                        .ok_or_else(|| format!("no link named {name}"))?;
                    serde_json::to_string_pretty(
                        &serde_json::json!({"name": o.name, "xml": pkg.object_text(o)?}),
                    )?
                }
            };
            println!("{json}");
        }
        Cmd::Export { package, dir } => {
            let pkg = Package::open(&package)?;
            for sub in ["tables", "queries", "forms", "reports", "macros", "modules"] {
                std::fs::create_dir_all(dir.join(sub))?;
            }
            let mut n = 0;
            let mut namer = Namer::new();
            for t in pkg.tables()? {
                let f = namer.file("tables", &t.name);
                std::fs::write(dir.join("tables").join(format!("{f}.csv")), t.to_csv())?;
                std::fs::write(
                    dir.join("tables").join(format!("{f}.schema.json")),
                    serde_json::to_vec_pretty(&t)?,
                )?;
                n += 1;
            }
            for q in pkg.queries()? {
                let f = namer.file("queries", &q.name);
                std::fs::write(
                    dir.join("queries").join(format!("{f}.sql")),
                    q.to_sql().text,
                )?;
                std::fs::write(dir.join("queries").join(format!("{f}.txt")), &q.text)?;
                n += 1;
            }
            for (sub, designs) in [("forms", pkg.forms()?), ("reports", pkg.reports()?)] {
                for d in designs {
                    let f = namer.file(sub, d.name());
                    std::fs::write(dir.join(sub).join(format!("{f}.txt")), d.text())?;
                    std::fs::write(
                        dir.join(sub).join(format!("{f}.json")),
                        serde_json::to_vec_pretty(&design_json(&d))?,
                    )?;
                    if let Some(code) = d.code_behind() {
                        std::fs::write(dir.join(sub).join(format!("{f}.bas")), code)?;
                    }
                    n += 1;
                }
            }
            for m in pkg.macros()? {
                let f = namer.file("macros", &m.name);
                std::fs::write(dir.join("macros").join(format!("{f}.txt")), &m.text)?;
                std::fs::write(
                    dir.join("macros").join(format!("{f}.json")),
                    serde_json::to_vec_pretty(&m)?,
                )?;
                n += 1;
            }
            for m in pkg.modules()? {
                let f = namer.file("modules", &m.name);
                std::fs::write(dir.join("modules").join(format!("{f}.bas")), &m.source)?;
                n += 1;
            }
            std::fs::write(
                dir.join("names.json"),
                serde_json::to_vec_pretty(&namer.manifest)?,
            )?;
            std::fs::write(
                dir.join("relationships.json"),
                serde_json::to_vec_pretty(&pkg.relationships()?)?,
            )?;
            std::fs::write(
                dir.join("database-properties.json"),
                serde_json::to_vec_pretty(&pkg.database_properties()?)?,
            )?;
            std::fs::write(
                dir.join("vba-references.json"),
                serde_json::to_vec_pretty(&pkg.vba_references()?)?,
            )?;
            println!("exported {n} objects to {}", dir.display());
        }
    }
    Ok(())
}
