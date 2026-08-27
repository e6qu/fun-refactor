//! Python reads a name out of a module object, and the import says which module.

use fun_refactor::index::Index;
use fun_refactor::model::Confidence;
use fun_refactor::scan::{scan, ScanOptions};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

/// The package, the flag module and one reader written the way the test names.
fn app(reader: &str) -> Vec<(String, String)> {
    vec![
        ("app/__init__.py".to_string(), String::new()),
        (
            "app/flags.py".to_string(),
            "USE_NEW_TAX = True\n".to_string(),
        ),
        ("app/tax.py".to_string(), reader.to_string()),
    ]
}

fn confidences(reader: &str) -> Vec<Confidence> {
    let owned = app(reader);
    let files: Vec<(&str, &str)> = owned
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let (_tmp, index) = workspace(&files);
    let found = index.find_symbols("USE_NEW_TAX", None);
    assert_eq!(found.len(), 1, "expected one declaration");
    index
        .references_to(found[0].id)
        .iter()
        .map(|r| r.confidence)
        .collect()
}

#[test]
fn a_submodule_imported_from_its_package_is_a_module() {
    assert_eq!(
        confidences("from app import flags\n\nx = flags.USE_NEW_TAX\n"),
        vec![Confidence::ImportQualified]
    );
}

#[test]
fn a_submodule_imported_under_an_alias_is_the_same_module() {
    assert_eq!(
        confidences("from app import flags as f\n\nx = f.USE_NEW_TAX\n"),
        vec![Confidence::ImportQualified]
    );
}

#[test]
fn a_dotted_import_is_read_by_its_dotted_path() {
    assert_eq!(
        confidences("import app.flags\n\nx = app.flags.USE_NEW_TAX\n"),
        vec![Confidence::ImportQualified]
    );
}

#[test]
fn a_relative_import_names_the_package_this_file_sits_in() {
    assert_eq!(
        confidences("from . import flags\n\nx = flags.USE_NEW_TAX\n"),
        vec![Confidence::ImportQualified]
    );
}

#[test]
fn a_relative_import_of_the_module_itself_still_binds_the_name() {
    assert_eq!(
        confidences("from .flags import USE_NEW_TAX\n\nx = USE_NEW_TAX\n"),
        vec![Confidence::ImportQualified, Confidence::ImportQualified]
    );
}

#[test]
fn a_name_declared_in_the_package_is_still_read_from_the_package() {
    // The submodule rule offers a second file.
    let (_tmp, index) = workspace(&[
        ("app/__init__.py", "LIMIT = 3\n"),
        ("app/use.py", "from app import LIMIT\n\nx = LIMIT\n"),
    ]);
    let found = index.find_symbols("LIMIT", None);
    assert_eq!(found.len(), 1);
    let confidences: Vec<Confidence> = index
        .references_to(found[0].id)
        .iter()
        .map(|r| r.confidence)
        .collect();
    assert_eq!(
        confidences,
        vec![Confidence::ImportQualified, Confidence::ImportQualified]
    );
}
