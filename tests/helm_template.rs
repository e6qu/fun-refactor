//! Go template parsing for Helm charts, and what it lets the analyses say.
//!
//! Two halves. The first parses action text directly: a template action is a small language.
//! The tests pin what each construct means and not what a substring happens to contain. The
//! second drives the public API over a realistic chart, subchart, `_helpers.tpl`, `include`, a
//! `{{- if }}`-wrapped block and a `{{ if }}`-guarded environment variable. Pins either the
//! resolved answer or the honest statement of what is left undecidable.

use fun_refactor::{
    analysis::provenance::{consumers, provenance, StopReason},
    analysis::{provenance as prov, stitch},
    helm::{self, ActionKind, Branch, Builtin, RefRoot, RegionKind},
    index::Index,
    lang::Language,
    model::{Symbol, SymbolId, SymbolKind},
    parse::Parsers,
    scan::{scan, ScanOptions},
    span::Span,
};

fn action(text: &str) -> helm::Action {
    helm::parse_action(text)
}

fn paths(text: &str) -> Vec<Vec<String>> {
    helm::values_paths_in(text)
}

fn path(segments: &[&str]) -> Vec<String> {
    segments.iter().map(|s| s.to_string()).collect()
}

/// Parse a whole file the way the analyses do: the action spans come from the parser that masks
/// them. So the two views can never drift apart.
fn template(source: &str) -> helm::Template {
    let parsed = Parsers::new()
        .parse(Language::Helm, source)
        .expect("helm parses");
    helm::Template::of(source, &parsed)
}

#[test]
fn a_values_path_survives_a_pipeline() {
    let a = action("{{ .Values.x | default \"y\" | quote }}");
    assert_eq!(a.kind, ActionKind::Expression);
    assert_eq!(a.values_paths(), vec![path(&["x"])]);
    assert_eq!(
        a.functions,
        vec!["default".to_string(), "quote".to_string()]
    );
    assert!(a.problems.is_empty(), "{:?}", a.problems);
}

#[test]
fn a_values_path_is_found_in_a_function_argument() {
    // The old substring scan found these too; the point is that the parser also
    // knows they are arguments, and which function they are arguments to.
    let a = action("{{ tpl .Values.tpl . }}");
    assert_eq!(a.values_paths(), vec![path(&["tpl"])]);
    assert_eq!(a.functions, vec!["tpl".to_string()]);
    assert!(
        a.refs.iter().any(|r| r.root == RefRoot::Dot),
        "the trailing dot is an argument in its own right: {:?}",
        a.refs
    );
}

#[test]
fn a_dotted_path_keeps_every_segment() {
    assert_eq!(paths("{{ .Values.a.b.c }}"), vec![path(&["a", "b", "c"])]);
    assert_eq!(
        paths("{{ printf \"%s:%s\" .Values.image.repository .Values.image.tag }}"),
        vec![path(&["image", "repository"]), path(&["image", "tag"])]
    );
}

#[test]
fn a_values_path_written_inside_a_string_is_not_a_values_path() {
    // A substring scan cannot tell these apart. The lexer can: one is a key, the
    // other is four words of prose.
    assert!(paths("{{ fail \"set .Values.image.tag before installing\" }}").is_empty());
    assert_eq!(
        paths("{{ .Values.image.tag | quote }}"),
        vec![path(&["image", "tag"])]
    );
    // A `}}` inside a string does not end the action either.
    let a = action("{{ printf \"}}\" .Values.a }}");
    assert_eq!(a.values_paths(), vec![path(&["a"])]);
}

#[test]
fn builtin_objects_are_told_apart_from_values() {
    let a = action("{{ .Release.Name }}-{{ .Chart.Version }}");
    assert_eq!(a.builtins(), vec![Builtin::Release, Builtin::Chart]);
    assert!(
        a.values_paths().is_empty(),
        "no values key named Release.Name will ever exist"
    );
    assert_eq!(helm::builtins_in("{{ .Release.Name }}"), vec![".Release"]);
    assert!(helm::builtins_in("{{ .Values.x }}").is_empty());

    for (text, expected) in [
        ("{{ .Release.Namespace }}", Builtin::Release),
        ("{{ .Chart.Name }}", Builtin::Chart),
        ("{{ .Capabilities.KubeVersion }}", Builtin::Capabilities),
        ("{{ .Files.Get \"config.ini\" }}", Builtin::Files),
        ("{{ .Template.BasePath }}", Builtin::Template),
        ("{{ .Subcharts.mysql.Values.x }}", Builtin::Subcharts),
    ] {
        assert_eq!(action(text).builtins(), vec![expected], "for {text}");
    }
}

#[test]
fn a_bare_builtin_with_no_field_still_counts() {
    // `.Release` alone reads the object; it is still not a values key.
    assert_eq!(
        action("{{ toYaml .Release }}").builtins(),
        vec![Builtin::Release]
    );
}

#[test]
fn trim_markers_are_recognised_by_gos_own_rule() {
    let both = action("{{- .Values.x -}}");
    assert!(both.trim_left && both.trim_right);
    assert_eq!(both.values_paths(), vec![path(&["x"])]);

    let left = action("{{- .Values.x }}");
    assert!(left.trim_left && !left.trim_right);

    let right = action("{{ .Values.x -}}");
    assert!(!right.trim_left && right.trim_right);

    // A hyphen with no space beside it is a minus sign. It is not a trim marker.
    let negative = action("{{-3}}");
    assert!(!negative.trim_left, "`{{-3}}` is the number -3");
    assert!(negative.problems.is_empty(), "{:?}", negative.problems);
}

#[test]
fn control_actions_are_classified_by_their_keyword() {
    assert_eq!(
        action("{{ if .Values.enabled }}").kind,
        ActionKind::If {
            expression: ".Values.enabled".to_string()
        }
    );
    assert_eq!(
        action("{{ else if eq .Values.env \"prod\" }}").kind,
        ActionKind::ElseIf {
            expression: "eq .Values.env \"prod\"".to_string()
        }
    );
    assert_eq!(action("{{ else }}").kind, ActionKind::Else);
    assert_eq!(action("{{- end }}").kind, ActionKind::End);
    assert_eq!(
        action("{{ with .Values.podAnnotations }}").kind,
        ActionKind::With {
            expression: ".Values.podAnnotations".to_string()
        }
    );
    assert_eq!(
        action("{{ define \"chart.name\" }}").kind,
        ActionKind::Define {
            name: "chart.name".to_string()
        }
    );
    assert_eq!(
        action("{{ template \"chart.name\" . }}").kind,
        ActionKind::TemplateCall {
            name: "chart.name".to_string()
        }
    );
    assert_eq!(
        action("{{ block \"body\" . }}").kind,
        ActionKind::Block {
            name: "body".to_string()
        }
    );
    assert_eq!(
        action("{{ $name := .Values.name }}").kind,
        ActionKind::Assignment {
            variable: "name".to_string()
        }
    );
    assert_eq!(action("{{/* a comment */}}").kind, ActionKind::Comment);
    assert_eq!(action("{{ }}").kind, ActionKind::Empty);
}

#[test]
fn a_range_reports_its_variables_and_its_subject() {
    let a = action("{{ range $index, $item := .Values.items }}");
    assert_eq!(
        a.kind,
        ActionKind::Range {
            expression: ".Values.items".to_string(),
            variables: vec!["index".to_string(), "item".to_string()],
        }
    );
    assert_eq!(a.values_paths(), vec![path(&["items"])]);

    let bare = action("{{ range .Values.items }}");
    assert_eq!(
        bare.kind,
        ActionKind::Range {
            expression: ".Values.items".to_string(),
            variables: Vec::new(),
        }
    );
}

#[test]
fn include_and_template_name_what_they_invoke() {
    let include = action("{{ include \"chart.labels\" . | nindent 4 }}");
    assert_eq!(include.invokes, vec!["chart.labels".to_string()]);
    assert_eq!(
        include.functions,
        vec!["include".to_string(), "nindent".to_string()]
    );

    assert_eq!(
        action("{{ template \"chart.name\" . }}").invokes,
        vec!["chart.name".to_string()]
    );
    // `block` defines and calls in the same breath.
    assert_eq!(
        action("{{ block \"body\" . }}").invokes,
        vec!["body".to_string()]
    );
    assert!(action("{{ .Values.x }}").invokes.is_empty());
}

#[test]
fn a_comment_holds_no_references() {
    let a = action("{{/* .Values.x is set by the operator */}}");
    assert_eq!(a.kind, ActionKind::Comment);
    assert!(a.values_paths().is_empty());
    assert!(a.refs.is_empty());
}

const NESTED: &str = r#"{{- if .Values.a }}
alpha: 1
{{- range .Values.list }}
beta: 2
{{- end }}
{{- else }}
gamma: 3
{{- end }}
"#;

#[test]
fn an_if_and_its_end_are_one_region() {
    let template = template(NESTED);
    assert!(template.unbalanced.is_empty(), "{:?}", template.unbalanced);
    assert_eq!(template.regions.len(), 2);
    assert_eq!(template.regions[0].kind, RegionKind::If);
    assert_eq!(template.regions[1].kind, RegionKind::Range);
    assert_eq!(template.regions[1].parent, Some(0));

    // The region covers exactly the bytes between the opener and its `end`.
    let body = template.regions[1].body.text(NESTED);
    assert_eq!(body.trim(), "beta: 2", "got {body:?}");
}

#[test]
fn a_conditional_names_itself_at_every_point_it_governs() {
    let template = template(NESTED);
    let describe = |needle: &str| -> Vec<String> {
        template
            .conditions_at(NESTED.find(needle).unwrap())
            .iter()
            .map(helm::Guard::describe)
            .collect()
    };
    assert_eq!(describe("alpha"), vec!["if .Values.a"]);
    assert_eq!(
        describe("beta"),
        vec!["if .Values.a", "range .Values.list"],
        "outermost first"
    );
    assert_eq!(describe("gamma"), vec!["the else branch of if .Values.a"]);
}

#[test]
fn an_else_if_names_its_own_condition() {
    let source = "{{- if .Values.a }}\nx: 1\n{{- else if .Values.b }}\ny: 2\n{{- else }}\nz: 3\n{{- end }}\n";
    let template = template(source);
    let guard = |needle: &str| template.conditions_at(source.find(needle).unwrap())[0].clone();

    assert_eq!(guard("x: 1").branch, Branch::Then);
    assert_eq!(
        guard("y: 2").branch,
        Branch::ElseIf(".Values.b".to_string())
    );
    assert_eq!(guard("z: 3").branch, Branch::Else);
    assert!(guard("y: 2").describe().contains("else if .Values.b"));
}

#[test]
fn an_unclosed_region_is_reported_rather_than_repaired() {
    // A Go template that does not balance does not render; saying so beats
    // pretending the file has a shape.
    let unclosed = template("{{- if .Values.a }}\nx: 1\n");
    assert_eq!(unclosed.unbalanced.len(), 1);
    assert!(unclosed.unbalanced[0].1.contains("never closed"));

    let stray = template("x: 1\n{{- end }}\n");
    assert_eq!(stray.unbalanced.len(), 1);
    assert!(stray.unbalanced[0].1.contains("closes nothing"));
}

#[test]
fn a_with_block_resolves_the_fields_written_under_it() {
    // `with` rebinds the dot to exactly one value, so `.annotations` under
    // `with .Values.pod` is `.Values.pod.annotations` and nothing else.
    let source = "{{- with .Values.pod }}\nannotations: {{ .annotations }}\n{{- end }}\n";
    let template = template(source);
    let index = template
        .actions
        .iter()
        .position(|a| a.text.contains(".annotations"))
        .unwrap();
    assert_eq!(
        template.values_paths_of(index),
        vec![path(&["pod", "annotations"])]
    );
}

#[test]
fn a_field_under_a_range_is_not_invented_as_a_values_key() {
    // The dot inside a `range` is an element of the collection; no values file has a key for
    // it. Guessing one would send provenance looking for nothing.
    let source = "{{- range .Values.hosts }}\n- host: {{ .name }}\n{{- end }}\n";
    let template = template(source);
    let index = template
        .actions
        .iter()
        .position(|a| a.text.contains(".name"))
        .unwrap();
    assert!(
        template.values_paths_of(index).is_empty(),
        "got {:?}",
        template.values_paths_of(index)
    );
    // The collection itself is a values key, and is reported.
    assert_eq!(template.values_paths(), vec![path(&["hosts"])]);
}

#[test]
fn the_root_context_reaches_past_a_rebound_dot() {
    // `$` is the root, so `$.Values.x` is a values key even inside a `range`.
    let source = "{{- range .Values.hosts }}\n- tag: {{ $.Values.image.tag }}\n{{- end }}\n";
    let template = template(source);
    assert_eq!(
        template.values_paths(),
        vec![path(&["hosts"]), path(&["image", "tag"])]
    );
}

#[test]
fn a_define_is_located_by_name_and_by_the_bytes_it_covers() {
    let source = concat!(
        "{{- define \"chart.name\" -}}\n",
        "{{ .Values.nameOverride }}\n",
        "{{- end }}\n",
        "{{- define \"chart.labels\" -}}\n",
        "name: {{ include \"chart.name\" . }}\n",
        "{{- end }}\n",
    );
    let template = template(source);
    assert_eq!(
        template
            .defines
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>(),
        vec!["chart.name", "chart.labels"]
    );

    let inside = source.find(".Values.nameOverride").unwrap();
    assert_eq!(
        template.define_containing(inside).map(|d| d.name.as_str()),
        Some("chart.name")
    );
    assert_eq!(
        template.invocations_of("chart.name").count(),
        1,
        "chart.labels includes chart.name"
    );
    assert_eq!(template.invocations_of("chart.absent").count(), 0);
}

#[test]
fn action_spans_index_the_original_file() {
    // Everything above is only usable if the spans still point at real bytes: the
    // masking in src/parse.rs is length-preserving precisely so they do.
    let source = "name: {{ .Values.x }}\n";
    let template = template(source);
    assert_eq!(template.actions.len(), 1);
    assert_eq!(template.actions[0].span.text(source), "{{ .Values.x }}");
    assert_eq!(template.actions[0].refs[0].span.text(source), ".Values.x");
    assert_eq!(
        template
            .action_at(source.find(".Values").unwrap())
            .map(|(i, _)| i),
        Some(0)
    );
    assert!(template.actions_in(Span::new(0, 5)).is_empty());
}

const CHART: &str = "name: app\nversion: 0.1.0\n";

const VALUES: &str = r#"nameOverride: parent-app
image:
  repository: parent
  tag: "1.0"
mysql:
  image:
    tag: "8.0"
ingress:
  enabled: true
  host: example.com
debug: false
logLevel: info
db:
  url: postgres://localhost
"#;

const SUBCHART_VALUES: &str = "image:\n  repository: mysql\n  tag: \"5.7\"\n";

const HELPERS: &str = r#"{{- define "app.fullname" -}}
{{- .Values.nameOverride | default .Chart.Name -}}
{{- end }}

{{- define "app.labels" -}}
app.kubernetes.io/name: {{ include "app.fullname" . }}
app.kubernetes.io/version: {{ .Values.image.tag | quote }}
{{- end }}
"#;

const DEPLOYMENT: &str = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include "app.fullname" . }}
  labels:
    {{- include "app.labels" . | nindent 4 }}
spec:
  template:
    spec:
      containers:
        - name: app
          image: "{{ .Values.image.repository }}:{{ .Values.image.tag }}"
          env:
            - name: DATABASE_URL
              value: {{ .Values.db.url | quote }}
            - name: LOG_LEVEL
{{- if .Values.debug }}
              value: debug
{{- else }}
              value: {{ .Values.logLevel }}
{{- end }}
"#;

const INGRESS: &str = r#"{{- if .Values.ingress.enabled }}
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: ingress
spec:
  rules:
    - host: {{ .Values.ingress.host }}
{{- end }}
"#;

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

fn chart() -> (tempfile::TempDir, Index) {
    workspace(&[
        ("app/Chart.yaml", CHART),
        ("app/values.yaml", VALUES),
        (
            "app/charts/mysql/Chart.yaml",
            "name: mysql\nversion: 8.0.0\n",
        ),
        ("app/charts/mysql/values.yaml", SUBCHART_VALUES),
        ("app/templates/_helpers.tpl", HELPERS),
        ("app/templates/deployment.yaml", DEPLOYMENT),
        ("app/templates/ingress.yaml", INGRESS),
        (
            "app/main.py",
            "import os\nos.environ[\"DATABASE_URL\"]\nos.getenv(\"LOG_LEVEL\")\n",
        ),
    ])
}

/// The key whose dotted path is `path`, in the file whose name ends with `file`.
fn key_with_path(index: &Index, file: &str, wanted: &str) -> SymbolId {
    let wanted: Vec<&str> = wanted.split('.').collect();
    let dotted = |s: &Symbol| {
        let mut s = s;
        let mut parts = vec![s.name.clone()];
        while let Some(id) = s.container {
            s = index.symbol(id).unwrap();
            parts.push(s.name.clone());
        }
        parts.reverse();
        parts
    };
    index
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Key && s.file.to_string_lossy().ends_with(file))
        .find(|s| dotted(s) == wanted)
        .unwrap_or_else(|| {
            panic!(
                "no key {wanted:?} in {file}; have {:?}",
                index
                    .symbols
                    .iter()
                    .filter(|s| s.file.to_string_lossy().ends_with(file))
                    .map(|s| dotted(s).join("."))
                    .collect::<Vec<_>>()
            )
        })
        .id
}

fn stops(result: &prov::Provenance) -> Vec<String> {
    result.stops.iter().map(|(_, r)| r.to_string()).collect()
}

#[test]
fn a_key_wrapped_in_an_if_is_reported_with_its_condition() {
    // B2: the masked YAML tree shows `host:` unconditionally, because the `{{- if }}`
    // around it became blank lines. The template parse is what knows better.
    let (_tmp, index) = chart();
    let host = key_with_path(&index, "ingress.yaml", "spec.rules.host");
    let result = provenance(&index, host, 6).unwrap();

    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::Conditional { what, condition }
                if what.contains("host") && condition == "if .Values.ingress.enabled"
        )),
        "got {:?}",
        stops(&result)
    );
    // And the condition's own values key is resolved. It is not named.
    assert!(
        result.hops.iter().any(|h| h.text.contains("enabled: true")),
        "got {:?}",
        result.hops.iter().map(|h| &h.text).collect::<Vec<_>>()
    );
}

#[test]
fn an_unguarded_key_is_not_reported_as_conditional() {
    // The other half of the same claim: only what is guarded is guarded.
    let (_tmp, index) = chart();
    let kind = key_with_path(&index, "deployment.yaml", "kind");
    let result = provenance(&index, kind, 4).unwrap();
    assert!(
        !result.stopped_because(|r| matches!(r, StopReason::Conditional { .. })),
        "got {:?}",
        stops(&result)
    );
}

#[test]
fn a_value_from_an_include_is_followed_into_the_define() {
    // B7: `{{ include "app.fullname" . }}` says nothing by itself. The named
    // template it calls is where `.Values.nameOverride` is read.
    let (_tmp, index) = chart();
    let name = key_with_path(&index, "deployment.yaml", "metadata.name");
    let result = provenance(&index, name, 8).unwrap();

    let texts: Vec<&str> = result.hops.iter().map(|h| h.text.as_str()).collect();
    assert!(
        texts.iter().any(|t| t.contains("define \"app.fullname\"")),
        "the define is a hop of its own: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("nameOverride: parent-app")),
        "the values key behind the define is resolved: {texts:?}"
    );
    assert!(
        result
            .hops
            .iter()
            .any(|h| h.file.to_string_lossy().ends_with("_helpers.tpl")),
        "the answer lives in another file, and says so: {:?}",
        result.hops.iter().map(|h| &h.file).collect::<Vec<_>>()
    );
    // `.Chart.Name` is the fallback, and comes from the release. It is not the workspace.
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { name, .. } if name == ".Chart"
        )),
        "got {:?}",
        stops(&result)
    );
}

#[test]
fn a_values_key_read_only_inside_a_define_names_its_include_sites() {
    // Forward: nothing includes `_helpers.tpl` textually. `.Values.image.tag` is
    // read there, and the places that read it are the places that include it.
    let (_tmp, index) = chart();
    let tag = key_with_path(&index, "app/values.yaml", "image.tag");
    let result = consumers(&index, tag, 6).unwrap();

    let texts: Vec<&str> = result.hops.iter().map(|h| h.text.as_str()).collect();
    assert!(
        texts
            .iter()
            .any(|t| t.contains("{{ .Values.image.tag | quote }}")),
        "the read inside the define: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("include \"app.labels\"")),
        "the include site that makes the read happen: {texts:?}"
    );
}

#[test]
fn a_read_under_a_conditional_says_which_condition() {
    let (_tmp, index) = chart();
    let enabled = key_with_path(&index, "app/values.yaml", "ingress.host");
    let result = consumers(&index, enabled, 6).unwrap();
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::Conditional { condition, .. } if condition == "if .Values.ingress.enabled"
        )),
        "got {:?}",
        stops(&result)
    );
}

#[test]
fn a_template_action_never_claims_more_than_a_name_match() {
    let (_tmp, index) = chart();
    let name = key_with_path(&index, "deployment.yaml", "metadata.name");
    let result = provenance(&index, name, 8).unwrap();
    assert!(result
        .hops
        .iter()
        .filter(|h| matches!(
            h.kind,
            prov::EdgeKind::TemplateAction | prov::EdgeKind::NamedTemplate
        ))
        .all(|h| !h.confidence.is_safe_to_rewrite()));
}

#[test]
fn an_include_of_a_template_nothing_defines_is_unresolved() {
    let (_tmp, index) = workspace(&[
        ("app/Chart.yaml", CHART),
        ("app/values.yaml", "a: 1\n"),
        (
            "app/templates/cm.yaml",
            "kind: ConfigMap\ndata:\n  x: {{ include \"app.missing\" . }}\n",
        ),
    ]);
    let key = key_with_path(&index, "cm.yaml", "data.x");
    let result = provenance(&index, key, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::Unresolved(m) if m.contains("app.missing") && m.contains("define")
        )),
        "got {:?}",
        stops(&result)
    );
}

#[test]
fn two_chart_values_files_have_a_decided_winner() {
    // B10, narrowed: `-f` and `--set` are invisible, but the order of a subchart's
    // values.yaml and its parent's is fixed by the chart hierarchy, so this one is
    // decided. Only the command-line-dependent part stays open.
    let (_tmp, index) = chart();
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = provenance(&index, tag, 5).unwrap();

    let competition = result
        .competitions
        .iter()
        .find(|c| c.subject.contains("image.tag"))
        .unwrap_or_else(|| panic!("no competition: {:?}", result.competitions));
    assert_eq!(competition.sources.len(), 2);
    assert!(
        competition.decided,
        "the parent chart outranks its subchart, and both are in the workspace"
    );
    assert!(competition.winner().unwrap().hop.text.contains("8.0"));
    assert!(
        !result.stopped_because(|r| matches!(r, StopReason::PrecedenceUndetermined(_))),
        "nothing here is undetermined: {:?}",
        stops(&result)
    );
    // The external channel is still reported. It replaces the answer instead of
    // reordering these two.
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { sources, .. } if sources.contains("--set")
        )),
        "got {:?}",
        stops(&result)
    );
}

#[test]
fn a_user_values_file_leaves_the_answer_to_the_command_line_and_says_so() {
    let (_tmp, index) = workspace(&[
        ("app/Chart.yaml", CHART),
        ("app/values.yaml", VALUES),
        ("app/values-prod.yaml", "image:\n  tag: \"2.0\"\n"),
    ]);
    let tag = key_with_path(&index, "app/values.yaml", "image.tag");
    let result = provenance(&index, tag, 5).unwrap();

    let competition = &result.competitions[0];
    assert!(!competition.decided);
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::PrecedenceUndetermined(m)
                if m.contains("-f values-prod.yaml") && m.contains("image.tag")
        )),
        "the message must name the input that would decide it: {:?}",
        stops(&result)
    );
}

#[test]
fn two_user_values_files_name_both_as_the_undecidable_pair() {
    let (_tmp, index) = workspace(&[
        ("app/Chart.yaml", CHART),
        ("app/values.yaml", "replicas: 1\n"),
        ("app/values-a.yaml", "replicas: 2\n"),
        ("app/values-b.yaml", "replicas: 3\n"),
    ]);
    let replicas = key_with_path(&index, "app/values.yaml", "replicas");
    let result = provenance(&index, replicas, 5).unwrap();

    assert!(result.competitions[0].winner().is_none());
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::PrecedenceUndetermined(m)
                if m.contains("values-a.yaml") && m.contains("values-b.yaml")
        )),
        "got {:?}",
        stops(&result)
    );
}

#[test]
fn an_env_var_takes_its_values_path_from_the_action_after_the_colon() {
    let (_tmp, index) = chart();
    let chains = stitch::chains(&index).unwrap();
    let chain = chains
        .iter()
        .find(|c| c.env_var == "DATABASE_URL")
        .unwrap_or_else(|| {
            panic!(
                "no chain: {:?}",
                chains.iter().map(|c| &c.env_var).collect::<Vec<_>>()
            )
        });

    assert_eq!(
        chain.values_path.as_deref(),
        Some(&path(&["db", "url"])[..])
    );
    assert!(chain.values_file.is_some());
    assert_eq!(chain.conditional_on, None, "this one is not guarded");
    assert_eq!(chain.reads.len(), 1, "got {:?}", chain.reads);
}

#[test]
fn a_guarded_env_var_yields_one_chain_per_branch_each_naming_its_condition() {
    // Both `value:` keys are siblings in the masked tree, one of them a lie for any
    // given render. Reporting a single winner would be the guess; reporting both,
    // each with the condition that selects it, is the answer.
    let (_tmp, index) = chart();
    let chains: Vec<_> = stitch::for_variable(&index, "LOG_LEVEL").unwrap();
    assert_eq!(chains.len(), 2, "got {chains:?}");

    let literal = chains
        .iter()
        .find(|c| c.values_path.is_none())
        .expect("the literal branch");
    assert_eq!(literal.conditional_on.as_deref(), Some("if .Values.debug"));

    let templated = chains
        .iter()
        .find(|c| c.values_path.is_some())
        .expect("the templated branch");
    assert_eq!(
        templated.values_path.as_deref(),
        Some(&path(&["logLevel"])[..])
    );
    assert_eq!(
        templated.conditional_on.as_deref(),
        Some("the else branch of if .Values.debug")
    );

    // Both branches still reach the program that reads the variable.
    assert!(chains.iter().all(|c| c.reads.len() == 1), "{chains:?}");
    let text = stitch::format_chains(&chains);
    assert!(text.contains("if .Values.debug"), "got:\n{text}");
}

#[test]
fn a_container_name_is_still_not_an_env_var() {
    let (_tmp, index) = chart();
    let chains = stitch::chains(&index).unwrap();
    assert!(
        !chains.iter().any(|c| c.env_var == "app"),
        "got {:?}",
        chains.iter().map(|c| &c.env_var).collect::<Vec<_>>()
    );
}
