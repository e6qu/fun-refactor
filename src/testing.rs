//! Fixtures the crate's own tests share.

use crate::index::Index;
use crate::scan::{scan, ScanOptions};

/// A workspace on disk, indexed.
pub fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        crate::vfs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

/// The same, from the files named rather than a walk of the directory.
pub fn indexed(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    let mut scanned = crate::scan::ScanResult::default();
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        crate::vfs::write(&path, content).unwrap();
        scanned.files.push(crate::scan::SourceFile {
            language: crate::lang::detect(&path).unwrap(),
            path,
        });
    }
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}
