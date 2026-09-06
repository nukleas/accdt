//! `accdt` command line: inspect and export Access template packages.

use std::path::PathBuf;
use std::process::ExitCode;

use accdt::{ObjectKind, Package};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "accdt", version, about = "Read Microsoft Access template packages (.accdt) without Access")]
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
    Cat { package: PathBuf, kind: String, name: String },
    /// Print an object as JSON (design controls, query definition, table schema, …)
    Json { package: PathBuf, kind: String, name: String },
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
    Ok(match s.to_ascii_lowercase().as_str() {
        "table" => ObjectKind::Table,
        "query" => ObjectKind::Query,
        "form" => ObjectKind::Form,
        "report" => ObjectKind::Report,
        "macro" => ObjectKind::Macro,
        "module" => ObjectKind::Module,
        other => return Err(format!("unknown kind {other}; use table|query|form|report|macro|module").into()),
    })
}

fn safe(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect()
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
                println!("required access version: {}  type: {}", t.required_access_version.unwrap_or_default(), t.template_type.unwrap_or_default());
            }
            for p in pkg.database_properties()? {
                if matches!(p.name.as_str(), "AccessVersion" | "AppTitle" | "StartUpForm" | "StartUpShowDBWindow" | "AllowBypassKey") {
                    println!("{}: {}", p.name, p.value);
                }
            }
            for k in [ObjectKind::Table, ObjectKind::Query, ObjectKind::Form, ObjectKind::Report, ObjectKind::Macro, ObjectKind::Module] {
                println!("{:<8} {}", k.as_str(), pkg.objects_of(k).count());
            }
            println!("relationships: {}", pkg.relationships()?.len());
            for r in pkg.vba_references()? {
                println!("reference: {} {}.{} {}{}", r.guid, r.major, r.minor, r.known_name().unwrap_or("?"), if r.is_32bit_only() { " [32-bit only]" } else { "" });
            }
        }
        Cmd::Ls { package } => {
            let pkg = Package::open(&package)?;
            for o in pkg.objects() {
                println!("{}\t{}", o.kind.as_str(), o.name);
            }
        }
        Cmd::Cat { package, kind: k, name } => {
            let pkg = Package::open(&package)?;
            let k = kind(&k)?;
            let o = pkg.object(k, &name).ok_or_else(|| format!("no {} named {name}", k.as_str()))?;
            match k {
                ObjectKind::Table => print!("{}", pkg.table(&name)?.unwrap().to_csv()),
                ObjectKind::Query => print!("{}", pkg.queries()?.into_iter().find(|q| q.name.eq_ignore_ascii_case(&name)).unwrap().to_sql().0),
                _ => print!("{}", pkg.object_text(o)?),
            }
        }
        Cmd::Json { package, kind: k, name } => {
            let pkg = Package::open(&package)?;
            let k = kind(&k)?;
            let o = pkg.object(k, &name).ok_or_else(|| format!("no {} named {name}", k.as_str()))?;
            let json = match k {
                ObjectKind::Table => serde_json::to_string_pretty(&pkg.table(&name)?.unwrap())?,
                ObjectKind::Query => serde_json::to_string_pretty(&pkg.queries()?.into_iter().find(|q| q.name.eq_ignore_ascii_case(&name)).unwrap())?,
                ObjectKind::Form => {
                    let d = pkg.form(&name)?.unwrap();
                    serde_json::to_string_pretty(&serde_json::json!({"name": d.name, "properties": d.properties, "controls": d.controls(), "events": d.events(), "embedded_macros": d.embedded_macros(), "code_behind": d.code_behind}))?
                }
                ObjectKind::Report => {
                    let d = pkg.report(&name)?.unwrap();
                    serde_json::to_string_pretty(&serde_json::json!({"name": d.name, "properties": d.properties, "controls": d.controls(), "events": d.events(), "embedded_macros": d.embedded_macros(), "code_behind": d.code_behind}))?
                }
                ObjectKind::Macro => serde_json::to_string_pretty(&pkg.macros()?.into_iter().find(|m| m.name.eq_ignore_ascii_case(&name)).unwrap())?,
                ObjectKind::Module => serde_json::to_string_pretty(&pkg.modules()?.into_iter().find(|m| m.name.eq_ignore_ascii_case(&name)).unwrap())?,
            };
            let _ = o;
            println!("{json}");
        }
        Cmd::Export { package, dir } => {
            let pkg = Package::open(&package)?;
            for sub in ["tables", "queries", "forms", "reports", "macros", "modules"] {
                std::fs::create_dir_all(dir.join(sub))?;
            }
            let mut n = 0;
            for t in pkg.tables()? {
                std::fs::write(dir.join("tables").join(format!("{}.csv", safe(&t.name))), t.to_csv())?;
                std::fs::write(dir.join("tables").join(format!("{}.schema.json", safe(&t.name))), serde_json::to_vec_pretty(&t)?)?;
                n += 1;
            }
            for q in pkg.queries()? {
                std::fs::write(dir.join("queries").join(format!("{}.sql", safe(&q.name))), q.to_sql().0)?;
                std::fs::write(dir.join("queries").join(format!("{}.txt", safe(&q.name))), &q.text)?;
                n += 1;
            }
            for (sub, designs) in [("forms", pkg.forms()?), ("reports", pkg.reports()?)] {
                for d in designs {
                    std::fs::write(dir.join(sub).join(format!("{}.txt", safe(&d.name))), &d.text)?;
                    let json = serde_json::json!({"name": d.name, "properties": d.properties, "controls": d.controls(), "events": d.events(), "embedded_macros": d.embedded_macros()});
                    std::fs::write(dir.join(sub).join(format!("{}.json", safe(&d.name))), serde_json::to_vec_pretty(&json)?)?;
                    if let Some(code) = &d.code_behind {
                        std::fs::write(dir.join(sub).join(format!("{}.bas", safe(&d.name))), code)?;
                    }
                    n += 1;
                }
            }
            for m in pkg.macros()? {
                std::fs::write(dir.join("macros").join(format!("{}.txt", safe(&m.name))), &m.text)?;
                std::fs::write(dir.join("macros").join(format!("{}.json", safe(&m.name))), serde_json::to_vec_pretty(&m)?)?;
                n += 1;
            }
            for m in pkg.modules()? {
                std::fs::write(dir.join("modules").join(format!("{}.bas", safe(&m.name))), &m.source)?;
                n += 1;
            }
            std::fs::write(dir.join("relationships.json"), serde_json::to_vec_pretty(&pkg.relationships()?)?)?;
            std::fs::write(dir.join("database-properties.json"), serde_json::to_vec_pretty(&pkg.database_properties()?)?)?;
            std::fs::write(dir.join("vba-references.json"), serde_json::to_vec_pretty(&pkg.vba_references()?)?)?;
            println!("exported {n} objects to {}", dir.display());
        }
    }
    Ok(())
}
