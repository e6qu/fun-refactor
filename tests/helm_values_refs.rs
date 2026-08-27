//! `.Values` paths are references, so a values key can be renamed.

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
    // `.Values.image.tag` names `tag` under `image`, not the unrelated `tag` beside it.
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

#[test]
fn a_value_in_two_values_files_renames_everywhere_at_once() {
    // values.yaml and values-prod.yaml declare one value; the rename has to move
    // both layers and the template read, or the chart splits in two.
    let tmp = chart(&[
        ("c/Chart.yaml", "apiVersion: v2\nname: c\nversion: 0.1.0\n"),
        ("c/values.yaml", "replicaCount: 1\n"),
        ("c/values-prod.yaml", "replicaCount: 5\n"),
        (
            "c/templates/deploy.yaml",
            "spec:\n  replicas: {{ .Values.replicaCount }}\n",
        ),
    ]);
    let index = index_of(&tmp);
    let key = index.find_symbols("replicaCount", Some(&tmp.path().join("c/values.yaml")))[0].id;

    let plan = fun_refactor::refactor::rename::plan(&index, key, "replicas").unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .unwrap();
    fun_refactor::edit::commit(&outcomes).unwrap();

    let defaults = std::fs::read_to_string(tmp.path().join("c/values.yaml")).unwrap();
    let prod = std::fs::read_to_string(tmp.path().join("c/values-prod.yaml")).unwrap();
    let template = std::fs::read_to_string(tmp.path().join("c/templates/deploy.yaml")).unwrap();
    assert!(defaults.contains("replicas: 1"), "got:\n{defaults}");
    assert!(prod.contains("replicas: 5"), "got:\n{prod}");
    assert!(
        template.contains("{{ .Values.replicas }}"),
        "got:\n{template}"
    );
}

#[test]
fn deleting_a_value_a_template_reads_is_refused_from_either_site() {
    let tmp = chart(&[
        ("c/Chart.yaml", "apiVersion: v2\nname: c\nversion: 0.1.0\n"),
        ("c/values.yaml", "replicaCount: 1\n"),
        ("c/values-prod.yaml", "replicaCount: 5\n"),
        (
            "c/templates/deploy.yaml",
            "spec:\n  replicas: {{ .Values.replicaCount }}\n",
        ),
    ]);
    let index = index_of(&tmp);
    for file in ["c/values.yaml", "c/values-prod.yaml"] {
        let key = index.find_symbols("replicaCount", Some(&tmp.path().join(file)))[0].id;
        let err = fun_refactor::refactor::delete::plan(&index, key)
            .expect_err("a value the template still reads must not be deletable");
        assert!(err.to_string().contains("refusing to delete"), "got: {err}");
    }
}

#[test]
fn deleting_a_value_nothing_reads_removes_every_layer() {
    let tmp = chart(&[
        ("c/Chart.yaml", "apiVersion: v2\nname: c\nversion: 0.1.0\n"),
        ("c/values.yaml", "replicaCount: 1\nkept: yes\n"),
        ("c/values-prod.yaml", "replicaCount: 5\n"),
        ("c/templates/deploy.yaml", "spec:\n  name: static\n"),
    ]);
    let index = index_of(&tmp);
    let key = index.find_symbols("replicaCount", Some(&tmp.path().join("c/values.yaml")))[0].id;

    let plan = fun_refactor::refactor::delete::plan(&index, key).unwrap();
    assert_eq!(plan.sites, 2, "both layers go together");
}
