//! Fixture loading shared by the integration tests.
//!
//! `northwind-2.0-dev.accdt` is on Microsoft's CDN, so a missing copy is something a
//! developer can fix and the test says how instead of passing quietly. Every other fixture
//! comes off the Access 2010 install media and cannot be downloaded, so those tests skip
//! when the file is absent. `ACCDT_SKIP_FIXTURE_TESTS=1` skips all of them.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use accdt::Package;

pub const NORTHWIND: &str = "northwind-2.0-dev.accdt";
pub const PROJECTS: &str = "project-management.accdt";

pub fn path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn skip_all() -> bool {
    std::env::var_os("ACCDT_SKIP_FIXTURE_TESTS").is_some()
}

/// A fixture that cannot be downloaded: `None` when it is not there.
pub fn optional(name: &str) -> Option<Package> {
    let p = path(name);
    (!skip_all() && p.exists()).then(|| Package::open(&p).expect(name))
}

/// A fixture `scripts/fetch-fixtures.sh` downloads: absent is an error, not a skip.
pub fn required(name: &str) -> Option<Package> {
    if skip_all() {
        return None;
    }
    let p = path(name);
    assert!(
        p.exists(),
        "fixture {} is missing; run scripts/fetch-fixtures.sh (or set ACCDT_SKIP_FIXTURE_TESTS=1)",
        p.display()
    );
    Some(Package::open(&p).expect(name))
}
