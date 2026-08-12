//! `.Values` paths are references, so a values key can be renamed.
//!
//! A Helm template action is masked before parsing. That is what keeps the surrounding YAML
//! parseable and the byte offsets honest, which left everything inside `{{ … }}` invisible to
//! the index. Provenance parsed the actions separately and could show which templates read a
//! key. But `fr refs` said zero and a rename of the key rewrote the values file and nothing
//! else. The paths are now extracted as references, scoped to their own chart.

use fun_refactor::index::Index;
use fun_refactor::model::Confidence;
use fun_refactor::scan::{scan, ScanOptions};

/// A chart with a values file and a template that reads it.
fn chart(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    tmp
}

fn index_of(tmp: &tempfile::TempDir) -> Index {
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    Index::build_from_scan(&scanned).unwrap()
}

#[test]
fn a_values_key_knows_which_templates_read_it() {
    let tmp = chart(&[
        ("c/Chart.yaml", "apiVersion: v2\nname: c\nversion: 0.1.0\n"),
        ("c/values.yaml", "appName: demo\nimage:\n  tag: v1\n"),
        (
            "c/templates/x.yaml",
            "a: \"{{ .Values.appName }}\"\nb: \"{{ .Values.image.tag }}\"\n",
        ),
    ]);
    let index = index_of(&tmp);

    let key = index.find_symbols("appName", None);
    assert_eq!(key.len(), 1, "the values key is indexed");
    let refs = index.references_to(key[0].id);
    assert_eq!(refs.len(), 1, "the template use is a reference: {refs:?}");
    assert_eq!(refs[0].confidence, Confidence::Exact);
}

#[test]
fn a_nested_values_path_does_not_match_the_same_leaf_elsewhere() {
    // `.Values.image.tag` names `tag` under `image`, not the unrelated `tag` beside
    // it. The segment before the key is what tells them apart.
    let tmp = chart(&[
        ("c/Chart.yaml", "apiVersion: v2\nname: c\nversion: 0.1.0\n"),
        ("c/values.yaml", "tag: standalone\nimage:\n  tag: v1\n"),
        ("c/templates/x.yaml", "b: \"{{ .Values.image.tag }}\"\n"),
    ]);
    let index = index_of(&tmp);

    let nested = index
        .symbols
        .iter()
        .find(|s| s.qualifier.as_deref() == Some("image"))
        .expect("image.tag is indexed");
    let standalone = index
        .symbols
        .iter()
        .find(|s| s.name == "tag" && s.qualifier.is_none())
        .expect("the top-level tag is indexed");

    assert_eq!(index.references_to(nested.id).len(), 1);
    assert!(
        index.references_to(standalone.id).is_empty(),
        "the top-level key is not what the path named"
    );
}

#[test]
fn one_chart_does_not_resolve_into_another_charts_values() {
    let tmp = chart(&[
        ("a/Chart.yaml", "apiVersion: v2\nname: a\nversion: 0.1.0\n"),
        ("a/values.yaml", "shared: from-a\n"),
        ("a/templates/x.yaml", "k: \"{{ .Values.shared }}\"\n"),
        ("b/Chart.yaml", "apiVersion: v2\nname: b\nversion: 0.1.0\n"),
        ("b/values.yaml", "shared: from-b\n"),
    ]);
    let index = index_of(&tmp);

    let a_key = index
        .symbols
        .iter()
        .find(|s| s.name == "shared" && s.file.starts_with(tmp.path().join("a")))
        .unwrap();
    let b_key = index
        .symbols
        .iter()
        .find(|s| s.name == "shared" && s.file.starts_with(tmp.path().join("b")))
        .unwrap();

    assert_eq!(index.references_to(a_key.id).len(), 1, "its own chart");
    assert!(
        index.references_to(b_key.id).is_empty(),
        "a neighbouring chart's key is a different key"
    );
}

#[test]
fn renaming_a_values_key_rewrites_the_templates_and_leaves_builtins_alone() {
    let tmp = chart(&[
        ("c/Chart.yaml", "apiVersion: v2\nname: c\nversion: 0.1.0\n"),
        ("c/values.yaml", "Name: demo\n"),
        (
            "c/templates/x.yaml",
            "a: \"{{ .Release.Name }}-{{ .Values.Name }}\"\n",
        ),
    ]);
    let index = index_of(&tmp);
    let key = index.find_symbols("Name", Some(&tmp.path().join("c/values.yaml")))[0].id;

    let plan = fun_refactor::refactor::rename::plan(&index, key, "appName").unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .unwrap();
    fun_refactor::edit::commit(&outcomes).unwrap();

    let template = std::fs::read_to_string(tmp.path().join("c/templates/x.yaml")).unwrap();
    assert!(
        template.contains("{{ .Values.appName }}"),
        "the values use moves:\n{template}"
    );
    assert!(
        template.contains("{{ .Release.Name }}"),
        "a builtin of the same name is not a values key:\n{template}"
    );
}
