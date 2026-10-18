#[path = "../src/native_fingerprint.rs"]
mod native_fingerprint;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("native-source-inputs.txt"),
        include_str!("../native-source-inputs.txt"),
    )
    .unwrap();
    for path in [
        "src",
        "grammars/python/src",
        "queries/python",
        "crates/local/src",
    ] {
        fs::create_dir_all(root.path().join(path)).unwrap();
    }
    for (path, content) in [
        ("src/rules.rs", "first rules"),
        ("grammars/python/src/parser.c", "first parser"),
        ("grammars/python/src/scanner.c", "first scanner"),
        ("queries/python/calls.scm", "first query"),
        ("crates/local/src/lib.rs", "first bridge"),
        ("Cargo.toml", "first manifest"),
        ("Cargo.lock", "first dependencies"),
        ("build.rs", "first builder"),
    ] {
        fs::write(root.path().join(path), content).unwrap();
    }
    root
}

fn digest(root: &Path) -> String {
    native_fingerprint::fingerprint(
        root,
        &BTreeMap::from([("compiler".into(), "first compiler".into())]),
    )
    .unwrap()
    .digest
}

#[test]
fn every_checking_input_changes_the_namespace() {
    for path in [
        "src/rules.rs",
        "grammars/python/src/parser.c",
        "grammars/python/src/scanner.c",
        "queries/python/calls.scm",
        "crates/local/src/lib.rs",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
    ] {
        let root = fixture();
        let before = digest(root.path());
        fs::write(root.path().join(path), "changed input").unwrap();
        assert_ne!(before, digest(root.path()), "{path}");
    }
}

#[test]
fn additions_deletions_and_renames_invalidate_without_a_manual_file_list() {
    let root = fixture();
    let before = digest(root.path());
    fs::write(root.path().join("src/new_rule.rs"), "new rule").unwrap();
    let added = digest(root.path());
    assert_ne!(added, before);
    fs::rename(
        root.path().join("src/new_rule.rs"),
        root.path().join("src/renamed_rule.rs"),
    )
    .unwrap();
    assert_ne!(added, digest(root.path()));
    fs::remove_file(root.path().join("src/renamed_rule.rs")).unwrap();
    assert_eq!(before, digest(root.path()));
    fs::remove_file(root.path().join("src/rules.rs")).unwrap();
    assert_ne!(before, digest(root.path()));
}

#[test]
fn build_settings_and_policy_are_inputs() {
    let root = fixture();
    let before = digest(root.path());
    for (key, value) in [
        ("compiler", "second compiler"),
        ("TARGET", "another target"),
        ("CARGO_FEATURE_LANG_PYTHON", "1"),
    ] {
        let settings = BTreeMap::from([
            ("compiler".into(), "first compiler".into()),
            (key.into(), value.into()),
        ]);
        assert_ne!(
            before,
            native_fingerprint::fingerprint(root.path(), &settings)
                .unwrap()
                .digest
        );
    }
    let policy = root.path().join("native-source-inputs.txt");
    fs::write(
        &policy,
        fs::read_to_string(&policy).unwrap() + "!extra_build_output/\n",
    )
    .unwrap();
    assert_ne!(before, digest(root.path()));
}

#[test]
fn generated_outputs_and_documentation_do_not_move_the_source_namespace() {
    let root = fixture();
    let before = digest(root.path());
    for path in [
        "target/binary",
        "src/target/cache",
        "grammars/python/node_modules/package/index.js",
        "docs/lesson.md",
    ] {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "generated or documented output").unwrap();
    }
    assert_eq!(before, digest(root.path()));
}

#[test]
fn missing_declared_inputs_and_malformed_policy_fail_loudly() {
    let root = fixture();
    fs::remove_file(root.path().join("Cargo.lock")).unwrap();
    let error = native_fingerprint::fingerprint(root.path(), &BTreeMap::new())
        .err()
        .unwrap();
    assert!(error.to_string().contains("Cargo.lock"));
    for policy in [
        "../outside\nnative-source-inputs.txt\n",
        "src/\nsrc/\nnative-source-inputs.txt\n",
        "src/\n",
        "src/\n\nnative-source-inputs.txt\n",
        "!bad/path/\nnative-source-inputs.txt\n",
    ] {
        let root = fixture();
        fs::write(root.path().join("native-source-inputs.txt"), policy).unwrap();
        assert!(native_fingerprint::fingerprint(root.path(), &BTreeMap::new()).is_err());
    }
}

#[test]
fn directories_are_watched_and_the_digest_is_sha256() {
    let root = fixture();
    let result = native_fingerprint::fingerprint(root.path(), &BTreeMap::new()).unwrap();
    assert_eq!(result.digest.len(), 64);
    assert!(result.digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(result.watched.contains(&"src".into()));
    assert!(result.watched.contains(&"grammars/python/src".into()));
    assert!(result
        .watched
        .contains(&"grammars/python/src/parser.c".into()));
}

#[cfg(unix)]
#[test]
fn symlinked_native_inputs_are_not_silently_skipped_or_followed() {
    let root = fixture();
    std::os::unix::fs::symlink("rules.rs", root.path().join("src/alias.rs")).unwrap();
    assert!(native_fingerprint::fingerprint(root.path(), &BTreeMap::new()).is_err());
}
