//! The vendor directory must describe itself accurately.
//!
//! Vendored material rots in two directions: a file changes and the manifest still
//! claims its old checksum, or a file appears with no provenance at all. Both are
//! failures of the same promise — that everything here can be traced to a source and
//! a licence — so both fail the build.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn vendor_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor")
}

fn manifest() -> String {
    std::fs::read_to_string(vendor_root().join("MANIFEST.toml"))
        .expect("vendor/MANIFEST.toml is readable")
}

/// Every `path = "..."` / `sha256 = "..."` pair, per language section.
fn recorded_files() -> Vec<(String, String, String)> {
    let text = manifest();
    let mut out = Vec::new();
    let mut language = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("language = ") {
            language = rest.trim_matches('"').to_string();
        }
        if line.starts_with("{ path =") {
            let path = line
                .split("path = \"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .expect("a path field");
            let digest = line
                .split("sha256 = \"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .expect("a sha256 field");
            out.push((language.clone(), path.to_string(), digest.to_string()));
        }
    }
    out
}

fn sha256_of(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    let digest = Sha256::digest(&bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn the_manifest_is_not_empty() {
    let files = recorded_files();
    assert!(
        files.len() >= 15,
        "expected a meaningful set of vendored queries, got {}",
        files.len()
    );
}

#[test]
fn every_recorded_file_exists_and_matches_its_checksum() {
    for (language, path, expected) in recorded_files() {
        let full = vendor_root()
            .join("tree-sitter-queries")
            .join(&language)
            .join(&path);
        assert!(
            full.exists(),
            "{language}/{path} is in the manifest but not on disk — \
             re-run `python3 vendor/vendor.py`"
        );
        assert_eq!(
            sha256_of(&full),
            expected,
            "{language}/{path} has changed since it was vendored. If that is \
             deliberate, re-run `python3 vendor/vendor.py` and read the diff: a \
             grammar that renamed a node is how queries/*/facts.scm silently stops \
             matching."
        );
    }
}

#[test]
fn every_vendored_file_has_provenance() {
    let recorded: BTreeSet<PathBuf> = recorded_files()
        .into_iter()
        .map(|(language, path, _)| {
            vendor_root()
                .join("tree-sitter-queries")
                .join(language)
                .join(path)
        })
        .collect();

    let mut stray = Vec::new();
    let queries = vendor_root().join("tree-sitter-queries");
    let mut stack = vec![queries.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "scm") && !recorded.contains(&path) {
                stray.push(path);
            }
        }
    }

    assert!(
        stray.is_empty(),
        "these files have no entry in MANIFEST.toml, so nothing records where they \
         came from or under what licence: {stray:?}"
    );
}

#[test]
fn every_licence_is_compatible_with_this_project() {
    // This project is AGPL-3.0-or-later. Permissive licences can be combined with it;
    // a differently-copylefted one cannot, and must not arrive unnoticed.
    const COMPATIBLE: &[&str] = &[
        "MIT",
        "Apache-2.0",
        "MIT OR Apache-2.0",
        "Apache-2.0 OR MIT",
        "BSD-3-Clause",
        "ISC",
        "Unlicense",
    ];

    let text = manifest();
    let mut checked = 0;
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("license = ") else {
            continue;
        };
        let licence = rest.trim_matches('"');
        checked += 1;
        assert!(
            COMPATIBLE.contains(&licence),
            "vendored material under '{licence}' — this project is \
             AGPL-3.0-or-later, so a licence outside the permissive set needs a \
             deliberate decision, not a silent import"
        );
    }
    assert!(checked > 0, "no licences were checked; the manifest looks wrong");
}

#[test]
fn a_licence_file_accompanies_every_grammar_that_ships_one() {
    let text = manifest();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("license_file = ") else {
            continue;
        };
        let relative = rest.trim_matches('"');
        assert!(
            vendor_root().join(relative).exists(),
            "{relative} is referenced by the manifest but missing — the licence text \
             has to travel with the files it covers"
        );
    }
}

#[test]
fn nothing_vendored_is_compiled_into_the_binary() {
    // Reference material and a compiled dependency carry different obligations. An
    // MIT file compiled into an AGPL binary needs its notice preserved; a file read
    // only by a maintainer does not. Keep the two apart.
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![src];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|e| e == "rs") {
                let source = std::fs::read_to_string(&path).unwrap_or_default();
                assert!(
                    !source.contains("vendor/"),
                    "{} reaches into vendor/ — that turns reference material into a \
                     compiled dependency, with different licence obligations",
                    path.display()
                );
            }
        }
    }
}
