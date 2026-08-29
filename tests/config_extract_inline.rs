//! Extract and inline variable for the config languages, end to end.

use fun_refactor::{
    edit::{apply_to_string, plan, Validation},
    index::Index,
    model::{SymbolId, SymbolKind},
    refactor::{extract, inline},
    scan::{scan, ScanOptions},
    span::Span,
};
use std::path::{Path, PathBuf};

struct Workspace {
    tmp: tempfile::TempDir,
    index: Index,
}

fn workspace(files: &[(&str, &str)]) -> Workspace {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    Workspace { tmp, index }
}

impl Workspace {
    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn reindex(&mut self) {
        let scanned = scan(self.tmp.path(), &ScanOptions::default()).unwrap();
        self.index = Index::build_from_scan(&scanned).unwrap();
    }

    /// The symbol of this kind whose name matches, anywhere in the workspace.
    fn symbol(&self, name: &str, kind: SymbolKind) -> SymbolId {
        self.index
            .find_symbols(name, None)
            .into_iter()
            .find(|s| s.kind == kind)
            .unwrap_or_else(|| {
                panic!(
                    "no {kind:?} named {name}: {:?}",
                    self.index
                        .symbols
                        .iter()
                        .map(|s| (&s.name, s.kind))
                        .collect::<Vec<_>>()
                )
            })
            .id
    }
}

/// Apply one file's edits to the text on disk.
fn applied(edits: &fun_refactor::edit::EditSet, path: &Path) -> String {
    let original = std::fs::read_to_string(path).unwrap_or_default();
    apply_to_string(&original, edits.edits_for(path).unwrap_or(&[])).unwrap()
}

/// Every planned file must still parse: the strongest guarantee the tool makes.
fn must_reparse(edits: &fun_refactor::edit::EditSet) {
    let outcomes = plan(edits, Validation::ReparseStrict).expect("edits must reparse cleanly");
    assert!(!outcomes.is_empty(), "a plan with no outcomes did nothing");
}

/// Write a plan's result back to disk so a follow-up refactoring sees it.
fn commit(edits: &fun_refactor::edit::EditSet) {
    for path in edits.paths() {
        let updated = applied(edits, path);
        std::fs::write(path, updated).unwrap();
    }
}

/// Every stretch of the original that no edit covers must reappear, in order and
/// byte-identical, in the result.
fn untouched_regions_survive(before: &str, after: &str, edits: &[fun_refactor::edit::Edit]) {
    let mut spans: Vec<Span> = edits.iter().map(|e| e.span).collect();
    spans.sort_by_key(|s| s.start);

    let mut pieces: Vec<&str> = Vec::new();
    let mut cursor = 0usize;
    for span in spans {
        if span.start > cursor {
            pieces.push(&before[cursor..span.start]);
        }
        cursor = cursor.max(span.end);
    }
    if cursor < before.len() {
        pieces.push(&before[cursor..]);
    }

    let mut at = 0usize;
    for piece in pieces {
        if piece.is_empty() {
            continue;
        }
        let found = after[at..].find(piece).unwrap_or_else(|| {
            panic!("untouched region {piece:?} is missing from the result:\n{after:?}")
        });
        at += found + piece.len();
    }
}

const TF_MAIN: &str = "\
resource \"aws_s3_bucket\" \"assets\" {
  bucket = \"acme-prod-assets\"
}

resource \"aws_s3_bucket\" \"logs\" {
  bucket = \"acme-prod-assets\"
}
";

#[test]
fn hcl_extract_creates_a_locals_block_when_the_file_has_none() {
    let ws = workspace(&[("main.tf", TF_MAIN)]);
    let path = ws.path("main.tf");
    let start = TF_MAIN.find("\"acme-prod-assets\"").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "\"acme-prod-assets\"".len()),
        "bucket_name",
        false,
    )
    .unwrap();

    assert_eq!(plan_out.occurrences, 1);
    assert_eq!(plan_out.expression, "\"acme-prod-assets\"");
    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
locals {
  bucket_name = \"acme-prod-assets\"
}

resource \"aws_s3_bucket\" \"assets\" {
  bucket = local.bucket_name
}

resource \"aws_s3_bucket\" \"logs\" {
  bucket = \"acme-prod-assets\"
}
"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn hcl_extract_replaces_every_occurrence_when_asked() {
    let ws = workspace(&[("main.tf", TF_MAIN)]);
    let path = ws.path("main.tf");
    let start = TF_MAIN.find("\"acme-prod-assets\"").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "\"acme-prod-assets\"".len()),
        "bucket_name",
        true,
    )
    .unwrap();

    assert_eq!(plan_out.occurrences, 2);
    let out = applied(&plan_out.edits, &path);
    assert_eq!(
        out,
        "\
locals {
  bucket_name = \"acme-prod-assets\"
}

resource \"aws_s3_bucket\" \"assets\" {
  bucket = local.bucket_name
}

resource \"aws_s3_bucket\" \"logs\" {
  bucket = local.bucket_name
}
"
    );
    assert_eq!(out.matches("\"acme-prod-assets\"").count(), 1);
    must_reparse(&plan_out.edits);
}

#[test]
fn hcl_extract_joins_an_existing_locals_block_and_leaves_comments_alone() {
    let src = "\
# top of file
locals {
  env = \"prod\"  # keep me
}

resource \"aws_s3_bucket\" \"b\" {
  bucket = \"acme\"
}
";
    let ws = workspace(&[("main.tf", src)]);
    let path = ws.path("main.tf");
    let start = src.find("\"acme\"").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "\"acme\"".len()),
        "bucket_name",
        false,
    )
    .unwrap();

    let out = applied(&plan_out.edits, &path);
    assert_eq!(
        out,
        "\
# top of file
locals {
  env = \"prod\"  # keep me
  bucket_name = \"acme\"
}

resource \"aws_s3_bucket\" \"b\" {
  bucket = local.bucket_name
}
"
    );
    assert!(out.contains("# top of file"));
    assert!(out.contains("# keep me"));
    must_reparse(&plan_out.edits);
}

#[test]
fn hcl_extract_refuses_a_name_the_module_already_uses() {
    // Terraform scopes to the directory, so a sibling file taking the name takes it.
    let ws = workspace(&[
        (
            "variables.tf",
            "variable \"region\" {\n  type = string\n}\n",
        ),
        (
            "main.tf",
            "resource \"aws_s3_bucket\" \"b\" {\n  bucket = \"acme\"\n}\n",
        ),
    ]);
    let path = ws.path("main.tf");
    let src = std::fs::read_to_string(&path).unwrap();
    let start = src.find("\"acme\"").unwrap();

    let err = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + 6),
        "region",
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("already defined"), "got: {err}");
    assert!(err.contains("variables.tf"), "got: {err}");
}

#[test]
fn hcl_extract_refuses_an_expression_that_is_already_a_local() {
    let src = "locals {\n  a = 1\n}\n\noutput \"o\" {\n  value = local.a\n}\n";
    let ws = workspace(&[("main.tf", src)]);
    let path = ws.path("main.tf");
    let start = src.find("local.a").unwrap();

    let err = extract::variable(&ws.index, &path, Span::new(start, start + 7), "b", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("already a named value"), "got: {err}");
}

#[test]
fn hcl_inline_substitutes_the_expression_and_removes_the_locals_block() {
    let src = "\
locals {
  bucket_name = \"acme-prod-assets\"
}

resource \"aws_s3_bucket\" \"assets\" {
  bucket = local.bucket_name
}
";
    let ws = workspace(&[("main.tf", src)]);
    let path = ws.path("main.tf");
    let id = ws.symbol("bucket_name", SymbolKind::Variable);

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(plan_out.use_sites, 1);
    assert_eq!(plan_out.value, "\"acme-prod-assets\"");
    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
resource \"aws_s3_bucket\" \"assets\" {
  bucket = \"acme-prod-assets\"
}
"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn hcl_inline_keeps_the_locals_block_when_other_entries_remain() {
    let src = "\
locals {
  env    = \"prod\"
  bucket = \"acme\"
}

output \"o\" {
  value = local.bucket
}
";
    let ws = workspace(&[("main.tf", src)]);
    let path = ws.path("main.tf");
    let id = ws.symbol("bucket", SymbolKind::Variable);

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
locals {
  env    = \"prod\"
}

output \"o\" {
  value = \"acme\"
}
"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn hcl_inline_reaches_across_files_in_the_same_module() {
    let ws = workspace(&[
        ("locals.tf", "locals {\n  prefix = \"acme\"\n}\n"),
        ("outputs.tf", "output \"o\" {\n  value = local.prefix\n}\n"),
    ]);
    let id = ws.symbol("prefix", SymbolKind::Variable);

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(plan_out.use_sites, 1);
    assert_eq!(applied(&plan_out.edits, &ws.path("locals.tf")), "");
    assert_eq!(
        applied(&plan_out.edits, &ws.path("outputs.tf")),
        "output \"o\" {\n  value = \"acme\"\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn hcl_inline_refuses_when_another_module_directory_uses_the_same_local() {
    // A local is module-scoped, and a module is a directory.
    let ws = workspace(&[
        ("locals.tf", "locals {\n  prefix = \"acme\"\n}\n"),
        ("outputs.tf", "output \"o\" {\n  value = local.prefix\n}\n"),
        (
            "child/main.tf",
            "output \"c\" {\n  value = local.prefix\n}\n",
        ),
    ]);
    let id = ws.symbol("prefix", SymbolKind::Variable);

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(
        err.contains("different Terraform module directory"),
        "got: {err}"
    );
    assert!(err.contains("child"), "got: {err}");
}

#[test]
fn hcl_inline_refuses_a_variable_block() {
    // `variable "x"` is a module input, not a local; changing it is a signature change.
    let ws = workspace(&[(
        "main.tf",
        "variable \"region\" {\n  default = \"eu-west-1\"\n}\n\noutput \"o\" {\n  value = var.region\n}\n",
    )]);
    let id = ws.symbol("region", SymbolKind::Variable);

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("not a `locals` entry"), "got: {err}");
}

#[test]
fn hcl_inline_refuses_an_attribute_read_on_the_local() {
    let src = "\
locals {
  cfg = { name = \"a\" }
}

output \"o\" {
  value = local.cfg.name
}
";
    let ws = workspace(&[("main.tf", src)]);
    let id = ws.symbol("cfg", SymbolKind::Variable);

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("read further"), "got: {err}");
}

#[test]
fn hcl_extract_then_inline_restores_the_original() {
    let mut ws = workspace(&[("main.tf", TF_MAIN)]);
    let path = ws.path("main.tf");
    let start = TF_MAIN.find("\"acme-prod-assets\"").unwrap();

    let extracted = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "\"acme-prod-assets\"".len()),
        "bucket_name",
        true,
    )
    .unwrap();
    commit(&extracted.edits);
    ws.reindex();

    let id = ws.symbol("bucket_name", SymbolKind::Variable);
    let inlined = inline::variable(&ws.index, id).unwrap();
    assert_eq!(applied(&inlined.edits, &path), TF_MAIN);
}

const VALUES: &str = "\
frontend:
  image: nginx:1.25
backend:
  image: nginx:1.25
";

#[test]
fn yaml_extract_anchors_the_first_occurrence_and_aliases_the_rest() {
    let ws = workspace(&[("values.yaml", VALUES)]);
    let path = ws.path("values.yaml");
    let start = VALUES.rfind("nginx:1.25").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "nginx:1.25".len()),
        "img",
        true,
    )
    .unwrap();

    assert_eq!(plan_out.occurrences, 2);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
frontend:
  image: &img nginx:1.25
backend:
  image: *img
"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn yaml_extract_anchors_the_first_occurrence_even_when_a_later_one_is_selected() {
    // YAML resolves an alias against an anchor already seen, so the anchor has to go
    // on the earliest occurrence whichever one the caller pointed at.
    let ws = workspace(&[("values.yaml", VALUES)]);
    let path = ws.path("values.yaml");
    let selected = VALUES.rfind("nginx:1.25").unwrap();
    let first = VALUES.find("nginx:1.25").unwrap();
    assert_ne!(selected, first);

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(selected, selected + "nginx:1.25".len()),
        "img",
        true,
    )
    .unwrap();
    let out = applied(&plan_out.edits, &path);
    assert!(
        out.contains("image: &img nginx:1.25\nbackend"),
        "got:\n{out}"
    );
    assert!(out.trim_end().ends_with("image: *img"), "got:\n{out}");
}

#[test]
fn yaml_extract_refuses_an_anchor_that_nothing_would_alias() {
    let ws = workspace(&[("values.yaml", "a: hello\nb: world\n")]);
    let path = ws.path("values.yaml");

    let err = extract::variable(&ws.index, &path, Span::new(3, 8), "greeting", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("written once"), "got: {err}");
}

#[test]
fn yaml_extract_of_a_repeated_value_asks_for_all() {
    let ws = workspace(&[("values.yaml", "a: hello\nb: hello\n")]);
    let path = ws.path("values.yaml");

    let err = extract::variable(&ws.index, &path, Span::new(3, 8), "greeting", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("--all"), "got: {err}");
    assert!(err.contains("1 other"), "got: {err}");

    let plan_out = extract::variable(&ws.index, &path, Span::new(3, 8), "greeting", true).unwrap();
    assert_eq!(
        applied(&plan_out.edits, &path),
        "a: &greeting hello\nb: *greeting\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn yaml_extract_refuses_a_key_rather_than_a_value() {
    let ws = workspace(&[("values.yaml", "a: hello\n")]);
    let path = ws.path("values.yaml");

    let err = extract::variable(&ws.index, &path, Span::new(0, 1), "x", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no anchorable scalar"), "got: {err}");
}

#[test]
fn yaml_extract_refuses_an_invalid_anchor_name() {
    let ws = workspace(&[("values.yaml", "a: hello\n")]);
    let path = ws.path("values.yaml");

    let err = extract::variable(&ws.index, &path, Span::new(3, 8), "not a name", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("not a valid name here"), "got: {err}");
}

#[test]
fn yaml_inline_substitutes_the_anchored_value_and_drops_the_sigil() {
    let src = "\
frontend:
  image: &img nginx:1.25
backend:
  image: *img
";
    let ws = workspace(&[("values.yaml", src)]);
    let path = ws.path("values.yaml");
    let id = ws.symbol("img", SymbolKind::Anchor);

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(plan_out.use_sites, 1);
    assert_eq!(plan_out.value, "nginx:1.25");
    assert_eq!(applied(&plan_out.edits, &path), VALUES);
    must_reparse(&plan_out.edits);
}

#[test]
fn yaml_inline_refuses_an_anchor_on_a_block_collection() {
    // The anchored node spans lines; splicing it at an alias would need re-indenting,
    // which is not a byte-preserving edit.
    let src = "\
defaults: &base
  retries: 3
  timeout: 30
service:
  <<: *base
";
    let ws = workspace(&[("values.yaml", src)]);
    let id = ws.symbol("base", SymbolKind::Anchor);

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("block collection"), "got: {err}");
    assert!(err.contains("re-indent"), "got: {err}");
}

#[test]
fn yaml_extract_then_inline_restores_the_original() {
    let mut ws = workspace(&[("values.yaml", VALUES)]);
    let path = ws.path("values.yaml");
    let start = VALUES.find("nginx:1.25").unwrap();

    let extracted = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "nginx:1.25".len()),
        "img",
        true,
    )
    .unwrap();
    commit(&extracted.edits);
    ws.reindex();

    let id = ws.symbol("img", SymbolKind::Anchor);
    let inlined = inline::variable(&ws.index, id).unwrap();
    assert_eq!(applied(&inlined.edits, &path), VALUES);
}

const HELM_CHART: &str = "apiVersion: v2\nname: acme\nversion: 0.1.0\n";

const HELM_DEPLOYMENT: &str = "\
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include \"acme.fullname\" . }}
spec:
  replicas: {{ .Values.replicaCount }}
  template:
    spec:
      containers:
        - name: main
          image: nginx:1.25
        - name: sidecar
          image: nginx:1.25
";

fn helm_workspace() -> Workspace {
    workspace(&[
        ("chart/Chart.yaml", HELM_CHART),
        ("chart/templates/deploy.yaml", HELM_DEPLOYMENT),
    ])
}

#[test]
fn helm_files_are_detected_as_helm() {
    let ws = helm_workspace();
    let info = ws
        .index
        .file(&ws.path("chart/templates/deploy.yaml"))
        .unwrap();
    assert_eq!(info.language, fun_refactor::lang::Language::Helm);
}

#[test]
fn helm_extract_anchors_a_scalar_outside_the_masked_spans() {
    let ws = helm_workspace();
    let path = ws.path("chart/templates/deploy.yaml");
    let start = HELM_DEPLOYMENT.find("nginx:1.25").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "nginx:1.25".len()),
        "img",
        true,
    )
    .unwrap();

    assert_eq!(plan_out.occurrences, 2);
    let out = applied(&plan_out.edits, &path);
    assert_eq!(
        out,
        "\
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include \"acme.fullname\" . }}
spec:
  replicas: {{ .Values.replicaCount }}
  template:
    spec:
      containers:
        - name: main
          image: &img nginx:1.25
        - name: sidecar
          image: *img
"
    );
    // The template actions are byte-identical: nothing touched them.
    assert!(out.contains("{{ include \"acme.fullname\" . }}"));
    assert!(out.contains("{{ .Values.replicaCount }}"));
    must_reparse(&plan_out.edits);
}

#[test]
fn helm_extract_refuses_a_selection_inside_a_masked_template_action() {
    let ws = helm_workspace();
    let path = ws.path("chart/templates/deploy.yaml");
    let start = HELM_DEPLOYMENT.find(".Values.replicaCount").unwrap();

    let err = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + ".Values.replicaCount".len()),
        "replicas",
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(
        err.contains("masked out before the YAML parse"),
        "got: {err}"
    );
    assert!(err.contains("{{ .Values.replicaCount }}"), "got: {err}");
}

#[test]
fn helm_extract_then_inline_restores_the_original() {
    let mut ws = helm_workspace();
    let path = ws.path("chart/templates/deploy.yaml");
    let start = HELM_DEPLOYMENT.find("nginx:1.25").unwrap();

    let extracted = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "nginx:1.25".len()),
        "img",
        true,
    )
    .unwrap();
    commit(&extracted.edits);
    ws.reindex();

    let id = ws.symbol("img", SymbolKind::Anchor);
    let inlined = inline::variable(&ws.index, id).unwrap();
    assert_eq!(applied(&inlined.edits, &path), HELM_DEPLOYMENT);
}

#[test]
fn helm_extract_function_writes_a_named_template_and_an_include() {
    let ws = helm_workspace();
    let path = ws.path("chart/templates/deploy.yaml");
    let region_start = HELM_DEPLOYMENT.find("        - name: main").unwrap();
    let region_end = HELM_DEPLOYMENT.find("        - name: sidecar").unwrap();

    let plan_out = extract::function(
        &ws.index,
        &path,
        Span::new(region_start, region_end),
        "container",
    )
    .unwrap();

    assert_eq!(plan_out.name, "acme.container");
    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include \"acme.fullname\" . }}
spec:
  replicas: {{ .Values.replicaCount }}
  template:
    spec:
      containers:
        {{ include \"acme.container\" . }}
        - name: sidecar
          image: nginx:1.25
"
    );

    let helpers = ws.path("chart/templates/_helpers.tpl");
    assert_eq!(
        applied(&plan_out.edits, &helpers),
        "\
{{- define \"acme.container\" -}}
        - name: main
          image: nginx:1.25
{{- end -}}
"
    );
    // The moved bytes are verbatim, indentation included.
    assert!(plan_out.body.starts_with("        - name: main\n"));
}

#[test]
fn helm_extract_function_refuses_without_a_chart_yaml() {
    // Without a Chart.yaml the chart name is unknown, and an include under the wrong
    // name renders empty instead of failing.
    let ws = workspace(&[("templates/deploy.yaml", "a: 1\nb: 2\n")]);
    let path = ws.path("templates/deploy.yaml");
    assert!(
        ws.index.file(&path).is_some(),
        "the template must be indexed"
    );

    let result = extract::function(&ws.index, &path, Span::new(0, 5), "thing");
    match result {
        Err(e) => {
            let text = e.to_string();
            assert!(
                text.contains("Chart.yaml") || text.contains("not supported for yaml"),
                "got: {text}"
            );
        }
        Ok(_) => panic!("a template with no Chart.yaml above it must be refused"),
    }
}

#[test]
fn helm_extract_function_writes_a_named_template_to_a_file_that_did_not_exist() {
    // `_helpers.tpl` normally has to be created, and `.tpl` had no language until
    // `Language::Helm` claimed the extension.
    let ws = helm_workspace();
    let path = ws.path("chart/templates/deploy.yaml");
    let region_start = HELM_DEPLOYMENT.find("        - name: main").unwrap();
    let region_end = HELM_DEPLOYMENT.find("        - name: sidecar").unwrap();

    let plan_out = extract::function(
        &ws.index,
        &path,
        Span::new(region_start, region_end),
        "container",
    )
    .unwrap();

    let outcomes = plan(&plan_out.edits, Validation::ReparseStrict)
        .expect("a destination that does not exist yet is an empty one");
    assert_eq!(outcomes.len(), 2, "the template file and the manifest");

    let helpers = outcomes
        .iter()
        .find(|o| o.path.ends_with("_helpers.tpl"))
        .expect("the named template is written");
    assert_eq!(
        helpers.updated,
        "{{- define \"acme.container\" -}}\n        - name: main\n          image: nginx:1.25\n{{- end -}}\n"
    );

    let manifest = outcomes
        .iter()
        .find(|o| o.path.ends_with("deploy.yaml"))
        .expect("the manifest is rewritten");
    assert!(
        manifest
            .updated
            .contains("{{ include \"acme.container\" . }}"),
        "got:\n{}",
        manifest.updated
    );
    // The sibling container is untouched.
    assert!(
        manifest.updated.contains("- name: sidecar"),
        "got:\n{}",
        manifest.updated
    );
}

const CSS: &str = "\
.btn {
  color: #3366ff;
}

.link {
  color: #3366ff;
}
";

#[test]
fn css_extract_creates_a_root_rule_and_var_uses() {
    let ws = workspace(&[("theme.css", CSS)]);
    let path = ws.path("theme.css");
    let start = CSS.find("#3366ff").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "#3366ff".len()),
        "brand",
        true,
    )
    .unwrap();

    assert_eq!(plan_out.name, "--brand");
    assert_eq!(plan_out.occurrences, 2);
    let out = applied(&plan_out.edits, &path);
    assert_eq!(
        out,
        "\
:root {
  --brand: #3366ff;
}

.btn {
  color: var(--brand);
}

.link {
  color: var(--brand);
}
"
    );
    assert_eq!(out.matches("#3366ff").count(), 1);
    must_reparse(&plan_out.edits);
}

#[test]
fn css_extract_joins_an_existing_root_rule_after_the_imports() {
    let src = "\
@import \"reset.css\";
:root {
  --gap: 4px;
}

.btn {
  color: #3366ff;
}
";
    let ws = workspace(&[("theme.css", src)]);
    let path = ws.path("theme.css");
    let start = src.find("#3366ff").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + 7),
        "--brand",
        false,
    )
    .unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
@import \"reset.css\";
:root {
  --gap: 4px;
  --brand: #3366ff;
}

.btn {
  color: var(--brand);
}
"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn css_extract_puts_a_new_root_rule_after_leading_at_rules() {
    let src = "@import \"reset.css\";\n.btn { color: red; }\n";
    let ws = workspace(&[("theme.css", src)]);
    let path = ws.path("theme.css");
    let start = src.find("red").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + 3),
        "brand",
        false,
    )
    .unwrap();

    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
@import \"reset.css\";
:root {
  --brand: red;
}

.btn { color: var(--brand); }
"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn scss_extract_produces_a_dollar_variable_at_the_top_level() {
    // A `$` name asks for an SCSS variable, which sits at the stylesheet's top level and
    // not in a `:root` rule: the compiler resolves a `$var`, never the cascade.
    let src = ".btn {\n  color: #3366ff;\n}\n\n.link {\n  color: #3366ff;\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");
    let start = src.find("#3366ff").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + 7),
        "$brand",
        true,
    )
    .unwrap();
    assert_eq!(plan_out.occurrences, 2);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "$brand: #3366ff;\n\n.btn {\n  color: $brand;\n}\n\n.link {\n  color: $brand;\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn scss_inline_substitutes_every_bare_use_and_removes_the_declaration() {
    let src =
        "$brand: #3366ff;\n\n.btn {\n  color: $brand;\n}\n\n.link {\n  border-color: $brand;\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");
    let symbol = ws
        .index
        .find_symbols("$brand", None)
        .first()
        .expect("the SCSS variable is a symbol")
        .id;

    let plan_out = inline::variable(&ws.index, symbol).unwrap();
    assert_eq!(plan_out.use_sites, 2);
    assert_eq!(
        applied(&plan_out.edits, &path),
        ".btn {\n  color: #3366ff;\n}\n\n.link {\n  border-color: #3366ff;\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn a_dollar_name_is_refused_in_plain_css() {
    // Plain CSS has no `$variable` syntax, so the request cannot be honoured there.
    let ws = workspace(&[("theme.css", ".btn { color: #3366ff; }\n")]);
    let path = ws.path("theme.css");
    let src = std::fs::read_to_string(&path).unwrap();
    let start = src.find("#3366ff").unwrap();

    let err = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + 7),
        "$brand",
        false,
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("plain CSS"), "got: {err}");
    assert!(err.contains("custom property"), "got: {err}");
}

#[test]
fn css_extract_works_alongside_scss_only_syntax() {
    // SCSS has its own grammar now, so a file using `$variables` parses cleanly and
    // extraction works in it instead of refusing.
    let src = "$brand: red;\n.btn { color: #3366ff; }\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");
    let start = src.find("#3366ff").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + 7),
        "accent",
        false,
    )
    .unwrap();
    let updated = applied(&plan_out.edits, &path);
    must_reparse(&plan_out.edits);
    assert!(updated.contains("--accent: #3366ff;"), "got:\n{updated}");
    assert!(updated.contains("var(--accent)"), "got:\n{updated}");
    // The SCSS-only line is untouched.
    assert!(updated.contains("$brand: red;"), "got:\n{updated}");
}

#[test]
fn css_extract_works_in_an_scss_file_that_is_css_compatible() {
    // The custom-property form is the one that serves both dialects.
    let src = ".btn {\n  color: #3366ff;\n}\n";
    let ws = workspace(&[("theme.scss", src)]);
    let path = ws.path("theme.scss");
    let start = src.find("#3366ff").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + 7),
        "brand",
        false,
    )
    .unwrap();
    assert_eq!(
        applied(&plan_out.edits, &path),
        ":root {\n  --brand: #3366ff;\n}\n\n.btn {\n  color: var(--brand);\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn css_inline_substitutes_every_var_use_and_removes_an_emptied_root() {
    let src = "\
:root {
  --brand: #3366ff;
}

.btn {
  color: var(--brand);
}
";
    let ws = workspace(&[("theme.css", src)]);
    let path = ws.path("theme.css");
    let id = ws.symbol("--brand", SymbolKind::Property);

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(plan_out.use_sites, 1);
    assert_eq!(plan_out.value, "#3366ff");
    assert_eq!(
        applied(&plan_out.edits, &path),
        ".btn {\n  color: #3366ff;\n}\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn css_inline_replaces_the_whole_var_call_including_its_fallback() {
    let src = ":root {\n  --brand: red;\n  --gap: 4px;\n}\n.btn { color: var(--brand, blue); }\n";
    let ws = workspace(&[("theme.css", src)]);
    let path = ws.path("theme.css");
    let id = ws.symbol("--brand", SymbolKind::Property);

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(
        applied(&plan_out.edits, &path),
        ":root {\n  --gap: 4px;\n}\n.btn { color: red; }\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn css_inline_refuses_a_property_declared_more_than_once() {
    // Which declaration wins at a use site is a cascade question.
    let src = ":root { --brand: red; }\n.dark { --brand: black; }\n.btn { color: var(--brand); }\n";
    let ws = workspace(&[("theme.css", src)]);
    let id = ws.symbol("--brand", SymbolKind::Property);

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("declared 2 times"), "got: {err}");
    assert!(err.contains("cascade"), "got: {err}");
}

#[test]
fn css_extract_then_inline_restores_the_original() {
    let mut ws = workspace(&[("theme.css", CSS)]);
    let path = ws.path("theme.css");
    let start = CSS.find("#3366ff").unwrap();

    let extracted =
        extract::variable(&ws.index, &path, Span::new(start, start + 7), "brand", true).unwrap();
    commit(&extracted.edits);
    ws.reindex();

    let id = ws.symbol("--brand", SymbolKind::Property);
    let inlined = inline::variable(&ws.index, id).unwrap();
    assert_eq!(applied(&inlined.edits, &path), CSS);
}

const MD: &str = "\
# Guide

Read the [reference](https://example.com/ref) first.

Then read the [reference](https://example.com/ref) again.
";

#[test]
fn markdown_extract_appends_a_link_reference_definition() {
    let ws = workspace(&[("guide.md", MD)]);
    let path = ws.path("guide.md");
    let start = MD.find("https://example.com/ref").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "https://example.com/ref".len()),
        "ref",
        false,
    )
    .unwrap();

    assert_eq!(plan_out.occurrences, 1);
    assert_eq!(plan_out.expression, "https://example.com/ref");
    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
# Guide

Read the [reference][ref] first.

Then read the [reference](https://example.com/ref) again.

[ref]: https://example.com/ref
"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn markdown_extract_rewrites_every_link_with_the_same_destination() {
    let ws = workspace(&[("guide.md", MD)]);
    let path = ws.path("guide.md");
    let start = MD.find("https://example.com/ref").unwrap();

    let plan_out = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "https://example.com/ref".len()),
        "ref",
        true,
    )
    .unwrap();

    assert_eq!(plan_out.occurrences, 2);
    let out = applied(&plan_out.edits, &path);
    assert_eq!(
        out,
        "\
# Guide

Read the [reference][ref] first.

Then read the [reference][ref] again.

[ref]: https://example.com/ref
"
    );
    assert!(out.starts_with("# Guide\n"), "the heading is untouched");
    must_reparse(&plan_out.edits);
}

#[test]
fn markdown_extract_beside_an_existing_definition_needs_no_blank_line() {
    let src = "See [a](/a) and [b][b].\n\n[b]: /b\n";
    let ws = workspace(&[("guide.md", src)]);
    let path = ws.path("guide.md");
    let start = src.find("/a").unwrap();

    let plan_out =
        extract::variable(&ws.index, &path, Span::new(start, start + 2), "a", false).unwrap();
    assert_eq!(
        applied(&plan_out.edits, &path),
        "See [a][a] and [b][b].\n\n[b]: /b\n[a]: /a\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn markdown_extract_refuses_a_reference_link() {
    // A reference link has no inline destination to promote.
    let src = "See [a][b].\n\n[b]: /b\n";
    let ws = workspace(&[("guide.md", src)]);
    let path = ws.path("guide.md");
    let start = src.find("[a][b]").unwrap() + 1;

    let err = extract::variable(&ws.index, &path, Span::new(start, start + 1), "c", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no inline `(destination)`"), "got: {err}");
}

#[test]
fn markdown_extract_refuses_a_label_already_defined() {
    let src = "See [a](/a).\n\n[ref]: /r\n";
    let ws = workspace(&[("guide.md", src)]);
    let path = ws.path("guide.md");
    let start = src.find("/a").unwrap();

    let err = extract::variable(&ws.index, &path, Span::new(start, start + 2), "ref", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("already defined"), "got: {err}");
}

#[test]
fn markdown_inline_rewrites_reference_links_and_removes_the_definition() {
    let src = "\
# Guide

Read the [reference][ref] first.

[ref]: https://example.com/ref
";
    let ws = workspace(&[("guide.md", src)]);
    let path = ws.path("guide.md");
    let id = ws.symbol("ref", SymbolKind::LinkDef);

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(plan_out.use_sites, 1);
    assert_eq!(plan_out.value, "https://example.com/ref");
    assert_eq!(
        applied(&plan_out.edits, &path),
        "\
# Guide

Read the [reference](https://example.com/ref) first.
"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn markdown_inline_handles_shortcut_and_collapsed_links_and_carries_the_title() {
    let src = "A [ref] and a [ref][] here.\n\n[ref]: /a \"Title\"\n";
    let ws = workspace(&[("guide.md", src)]);
    let path = ws.path("guide.md");
    let id = ws.symbol("ref", SymbolKind::LinkDef);

    let plan_out = inline::variable(&ws.index, id).unwrap();
    assert_eq!(plan_out.use_sites, 2);
    assert_eq!(
        applied(&plan_out.edits, &path),
        "A [ref](/a \"Title\") and a [ref](/a \"Title\") here.\n"
    );
    must_reparse(&plan_out.edits);
}

#[test]
fn markdown_inline_refuses_a_definition_with_no_uses() {
    let src = "# Guide\n\n[orphan]: /a\n";
    let ws = workspace(&[("guide.md", src)]);
    let id = ws.symbol("orphan", SymbolKind::LinkDef);

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("no reference links"), "got: {err}");
}

#[test]
fn markdown_inline_refuses_a_use_in_another_document() {
    // Link reference definitions are document-scoped; a `[ref]` elsewhere is not this.
    let ws = workspace(&[
        ("a.md", "See [x][ref].\n\n[ref]: /a\n"),
        ("b.md", "Also [y][ref].\n"),
    ]);
    let id = ws.symbol("ref", SymbolKind::LinkDef);

    let err = inline::variable(&ws.index, id).unwrap_err().to_string();
    assert!(err.contains("document that contains it"), "got: {err}");
    assert!(err.contains("b.md"), "got: {err}");
}

#[test]
fn markdown_extract_then_inline_restores_the_original() {
    let mut ws = workspace(&[("guide.md", MD)]);
    let path = ws.path("guide.md");
    let start = MD.find("https://example.com/ref").unwrap();

    let extracted = extract::variable(
        &ws.index,
        &path,
        Span::new(start, start + "https://example.com/ref".len()),
        "ref",
        true,
    )
    .unwrap();
    commit(&extracted.edits);
    ws.reindex();

    let id = ws.symbol("ref", SymbolKind::LinkDef);
    let inlined = inline::variable(&ws.index, id).unwrap();
    assert_eq!(applied(&inlined.edits, &path), MD);
}

#[test]
fn every_config_extraction_leaves_the_rest_of_the_file_byte_identical() {
    // The edited ranges are the only ones that move.
    let src = "# a comment\nlocals {\n  keep = \"me\"\n}\n\n\nresource \"aws_s3_bucket\" \"b\" {\n  bucket   =    \"acme\"\n  tags = {}\n}\n   \n";
    let ws = workspace(&[("main.tf", src)]);
    let path = ws.path("main.tf");
    let start = src.find("\"acme\"").unwrap();

    let plan_out =
        extract::variable(&ws.index, &path, Span::new(start, start + 6), "name", false).unwrap();
    let out = applied(&plan_out.edits, &path);

    assert!(out.contains("# a comment\n"));
    assert!(out.contains("  bucket   =    local.name\n"));
    assert!(
        out.contains("\n\n\nresource"),
        "blank lines survive:\n{out:?}"
    );
    assert!(
        out.ends_with("}\n   \n"),
        "trailing spaces survive:\n{out:?}"
    );
    untouched_regions_survive(src, &out, plan_out.edits.edits_for(&path).unwrap());
    must_reparse(&plan_out.edits);
}

#[test]
fn untouched_regions_survive_every_config_extraction() {
    // The same property, checked against the edit spans themselves, for each language.
    let cases: &[(&str, &str, &str, &str)] = &[
        ("main.tf", TF_MAIN, "\"acme-prod-assets\"", "bucket_name"),
        ("values.yaml", VALUES, "nginx:1.25", "img"),
        ("theme.css", CSS, "#3366ff", "brand"),
        ("guide.md", MD, "https://example.com/ref", "ref"),
    ];
    for (name, src, needle, new_name) in cases {
        let ws = workspace(&[(name, src)]);
        let path = ws.path(name);
        let start = src.find(needle).unwrap();
        let plan_out = extract::variable(
            &ws.index,
            &path,
            Span::new(start, start + needle.len()),
            new_name,
            true,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        let out = applied(&plan_out.edits, &path);
        untouched_regions_survive(src, &out, plan_out.edits.edits_for(&path).unwrap());
        must_reparse(&plan_out.edits);
    }
}

#[test]
fn html_remains_refused_for_extract_variable() {
    // HTML has no binding form at all: a reusable value there is a CSS custom property, which
    // belongs to the stylesheet.
    let ws = workspace(&[("page.html", "<div id=\"main\">hello</div>\n")]);
    let path = ws.path("page.html");
    let err = extract::variable(&ws.index, &path, Span::new(1, 4), "x", false)
        .unwrap_err()
        .to_string();
    assert!(err.contains("not supported for"), "got: {err}");
}

#[test]
fn a_go_binding_is_placed_before_the_statement_it_serves() {
    // Go puts a `statement_list` between a block and its statements.
    let src = "package p\n\nfunc f(xs []int) {\n\titems := []int{}\n\
               \tfor _, x := range xs {\n\t\titems = append(items, x)\n\t}\n\
               \tif len(items) > 0 {\n\t\tuse(items)\n\t}\n}\n\nfunc use(x []int) {}\n";
    let ws = workspace(&[("a.go", src)]);
    let path = ws.path("a.go");

    let start = src.find("len(items)").unwrap();
    let span = Span::new(start, start + "len(items)".len());
    let plan = extract::variable(&ws.index, &path, span, "count", false).unwrap();
    let out = apply_to_string(src, plan.edits.edits_for(&path).unwrap()).unwrap();

    let binding = out.find("count := len(items)").expect("the binding exists");
    let declaration = out
        .find("items := []int{}")
        .expect("the declaration exists");
    assert!(
        binding > declaration,
        "the binding reads `items`, so it cannot precede it:\n{out}"
    );
    assert!(
        out.contains("\tcount := len(items)\n\tif count > 0 {"),
        "it belongs immediately before the statement that uses it:\n{out}"
    );
}
