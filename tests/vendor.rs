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
    let mut seen = 0;
    let queries = vendor_root().join("tree-sitter-queries");
    let mut stack = vec![queries.clone()];
    while let Some(dir) = stack.pop() {
        // A directory that cannot be read is not a directory with nothing in it. The
        // whole walk used to pass by finding nothing, so a vendor tree that had gone
        // missing read as a vendor tree with no stray files in it.
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{} cannot be read: {e}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "scm") {
                seen += 1;
                if !recorded.contains(&path) {
                    stray.push(path);
                }
            }
        }
    }

    assert!(
        seen > 0,
        "no vendored query files were found under {}, so this checked nothing",
        queries.display()
    );

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
    assert!(
        checked > 0,
        "no licences were checked; the manifest looks wrong"
    );
}

#[test]
fn a_licence_file_accompanies_every_grammar_that_ships_one() {
    let text = manifest();
    let mut checked = 0;
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("license_file = ") else {
            continue;
        };
        let relative = rest.trim_matches('"');
        checked += 1;
        assert!(
            vendor_root().join(relative).exists(),
            "{relative} is referenced by the manifest but missing — the licence text \
             has to travel with the files it covers"
        );
    }
    assert!(
        checked > 0,
        "the manifest names no licence file at all, so this checked nothing"
    );
}

#[test]
fn nothing_vendored_is_compiled_into_the_binary() {
    // Reference material and a compiled dependency carry different obligations. An
    // MIT file compiled into an AGPL binary needs its notice preserved; a file read
    // only by a maintainer does not. Keep the two apart.
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![src];
    let mut read = 0;
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{} cannot be read: {e}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|e| e == "rs") {
                // Not `unwrap_or_default`: a file that cannot be read became empty
                // source, and empty source contains no `vendor/`.
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("{} cannot be read: {e}", path.display()));
                read += 1;
                assert!(
                    !source.contains("vendor/"),
                    "{} reaches into vendor/ — that turns reference material into a \
                     compiled dependency, with different licence obligations",
                    path.display()
                );
            }
        }
    }
    assert!(
        read > 10,
        "only {read} source file(s) were read; the walk found nothing"
    );
}

// ------------------------------------------------------- the translation corpus

/// `tests/corpus/` — real files from real projects, kept for the translation tests.
///
/// Described by `PROVENANCE.md` instead of a manifest, because it is a document a
/// person reads. That makes it exactly as rot-prone as `MANIFEST.toml` and it needs
/// the same check: a file that changes without its checksum changing is a file whose
/// provenance is a claim and not a fact.
fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("corpus")
}

/// Every `| name | sha256 | origin |` row in the provenance document.
fn corpus_checksums() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(corpus_root().join("PROVENANCE.md"))
        .expect("tests/corpus/PROVENANCE.md is readable");
    let mut out = Vec::new();
    for line in text.lines() {
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
        if cells.len() < 3 {
            continue;
        }
        let name = cells[0].trim().trim_matches('`');
        let digest = cells[1].trim().trim_matches('`');
        if digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit()) {
            out.push((name.to_string(), digest.to_string()));
        }
    }
    out
}

fn corpus_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![corpus_root()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .expect("the corpus is readable")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e != "md") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_corpus_file_is_recorded_and_matches_its_checksum() {
    let recorded = corpus_checksums();
    assert!(
        recorded.len() >= 11,
        "the provenance document lists {} files",
        recorded.len()
    );
    for file in corpus_files() {
        let relative = file
            .strip_prefix(corpus_root())
            .expect("under the corpus")
            .to_string_lossy()
            .to_string();
        let name = file
            .file_name()
            .expect("a name")
            .to_string_lossy()
            .to_string();
        // A row names either the file or its path under the project directory, since
        // the Next.js tree's shape is part of what is vendored.
        let row = recorded
            .iter()
            .find(|(recorded, _)| *recorded == name || relative.ends_with(recorded.as_str()))
            .unwrap_or_else(|| {
                panic!("{relative} is vendored with no row in tests/corpus/PROVENANCE.md")
            });
        assert_eq!(
            row.1,
            sha256_of(&file),
            "{relative} has changed since it was vendored, or its checksum was mistyped"
        );
    }
}

#[test]
fn every_corpus_project_names_a_licence_that_permits_redistribution() {
    let text = std::fs::read_to_string(corpus_root().join("PROVENANCE.md"))
        .expect("tests/corpus/PROVENANCE.md is readable");
    // One per `## name/` section, since a section is one upstream project.
    let projects = text.matches("\n- Source: ").count();
    let licences = text.matches("\n- License: ").count();
    assert_eq!(
        projects, licences,
        "every vendored project needs a source and a licence"
    );
    assert!(projects >= 4, "found {projects} vendored projects");
    for line in text.lines().filter(|l| l.starts_with("- License: ")) {
        assert!(
            line.contains("MIT") || line.contains("Apache-2.0") || line.contains("BSD"),
            "a licence that may not permit redistribution: {line}"
        );
    }
}
