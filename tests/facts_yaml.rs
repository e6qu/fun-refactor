//! YAML and Helm fact extraction, exercised through the public API.
//!
//! Two things are being pinned down here. For plain YAML: mapping keys are the
//! API of a values file, so every key is a definition whose containment gives it
//! a path, and anchor/alias is the one reference edge YAML resolves within a
//! single file. For Helm: `{{ ... }}` actions are masked out before the
//! YAML parse (src/parse.rs), so these tests establish that the surrounding
//! document still extracts cleanly and that nothing from inside a template leaks
//! out as a bogus symbol.

use fun_refactor::{extract::Extractor, lang::Language, model::*, parse::Parsers};
use std::path::Path;

fn facts(lang: Language, src: &str) -> FileFacts {
    let parsed = Parsers::new().parse(lang, src).unwrap();
    Extractor::new()
        .extract(&parsed, Path::new("t"), src)
        .unwrap()
}

fn yaml(src: &str) -> FileFacts {
    facts(Language::Yaml, src)
}

fn helm(src: &str) -> FileFacts {
    facts(Language::Helm, src)
}

fn names(f: &FileFacts) -> Vec<&str> {
    f.symbols.iter().map(|s| s.name.as_str()).collect()
}

fn qualified(f: &FileFacts) -> Vec<String> {
    f.symbols.iter().map(|s| s.qualified_name()).collect()
}

fn sym<'a>(f: &'a FileFacts, qualified_name: &str) -> &'a Symbol {
    f.symbols
        .iter()
        .find(|s| s.qualified_name() == qualified_name)
        .unwrap_or_else(|| panic!("no symbol {qualified_name}: {:?}", qualified(f)))
}

const VALUES: &str = r#"defaults: &base
  retries: 3
  timeout: 30

image:
  repository: nginx
  tag: "1.25"

service:
  <<: *base
  type: ClusterIP
  ports:
    - name: http
      port: 80

flow: { a: 1, b: [2, 3] }
ref: *base
"quoted key": value
scalarAnchor: &n 7
"#;

const HELM_DEPLOYMENT: &str = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include "chart.fullname" . }}
  labels:
    app: {{ .Values.appName }}
spec:
  replicas: {{ .Values.replicaCount }}
  template:
    spec:
      containers:
        - name: main
          image: "{{ .Values.image.repository }}:{{ .Values.image.tag }}"
          {{- if .Values.resources }}
          resources:
            limits:
              cpu: {{ .Values.resources.cpu }}
          {{- end }}
"#;

// --------------------------------------------------------------- plain YAML

#[test]
fn values_file_parses_cleanly() {
    let parsed = Parsers::new().parse(Language::Yaml, VALUES).unwrap();
    assert!(
        !parsed.has_errors(),
        "values.yaml should parse without errors: {:?}",
        parsed.error_spans()
    );
}

#[test]
fn every_mapping_key_is_a_definition() {
    let f = yaml(VALUES);
    for key in [
        "defaults",
        "retries",
        "image",
        "repository",
        "tag",
        "service",
        "ports",
    ] {
        let hits: Vec<_> = f.symbols.iter().filter(|s| s.name == key).collect();
        assert_eq!(hits.len(), 1, "{key}: got {hits:?}");
        assert_eq!(hits[0].kind, SymbolKind::Key);
    }
}

#[test]
fn key_name_span_is_the_key_alone() {
    // A rename must rewrite the key text and neither the colon nor the value.
    let f = yaml(VALUES);
    let tag = sym(&f, "image::tag");
    assert_eq!(tag.name_span.text(VALUES), "tag");
    assert_eq!(VALUES.as_bytes()[tag.name_span.end], b':');
    // The full span is the whole pair, so deleting an unused key takes its value.
    assert_eq!(tag.full_span.text(VALUES), "tag: \"1.25\"");
    assert!(tag.full_span.contains(tag.name_span));
}

#[test]
fn nested_keys_are_qualified_by_their_parent_key() {
    // One level of qualification, which is what the engine's `qualifier` holds.
    let f = yaml(VALUES);
    assert_eq!(sym(&f, "image::tag").name, "tag");
    assert_eq!(
        sym(&f, "image::repository").qualifier.as_deref(),
        Some("image")
    );
    assert_eq!(
        sym(&f, "defaults::retries").qualifier.as_deref(),
        Some("defaults")
    );
    // Top-level keys have no qualifier at all.
    assert_eq!(sym(&f, "image").qualifier, None);
}

#[test]
fn deeper_paths_are_walked_through_the_container_chain() {
    // `qualifier` is a single level by design; the full dotted path comes from
    // following `Symbol::container` outwards.
    let f = yaml(VALUES);
    let port = sym(&f, "ports::port");
    let mut path = vec![port.name.clone()];
    let mut cur = port;
    while let Some(id) = cur.container {
        cur = f.symbol(id).unwrap();
        path.push(cur.name.clone());
    }
    path.reverse();
    assert_eq!(path, vec!["service", "ports", "port"], "got {path:?}");
}

#[test]
fn keys_under_a_sequence_are_qualified_by_the_sequence_key() {
    let f = yaml(VALUES);
    assert_eq!(sym(&f, "ports::name").name, "name");
    assert_eq!(sym(&f, "ports::port").name, "port");
}

#[test]
fn flow_mapping_keys_are_definitions_too() {
    let f = yaml(VALUES);
    assert_eq!(sym(&f, "flow::a").kind, SymbolKind::Key);
    assert_eq!(sym(&f, "flow::b").name_span.text(VALUES), "b");
}

// ------------------------------------------------------------ anchor/alias

#[test]
fn anchor_and_alias_pair_up_by_name() {
    // This is the YAML rename story: renaming `&base` must rewrite every `*base`.
    let f = yaml(VALUES);
    let anchor = f
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Anchor && s.name == "base")
        .unwrap_or_else(|| panic!("no anchor `base`: {:?}", names(&f)));
    assert_eq!(anchor.name_span.text(VALUES), "base");
    // The `&` sigil is not part of the name span, so a rename leaves it alone.
    assert_eq!(VALUES.as_bytes()[anchor.name_span.start - 1], b'&');

    let aliases: Vec<_> = f.references.iter().filter(|r| r.name == "base").collect();
    assert_eq!(aliases.len(), 2, "got {aliases:?}");
    for a in &aliases {
        assert_eq!(a.kind, ReferenceKind::Identifier);
        assert_eq!(a.span.text(VALUES), "base");
        assert_eq!(VALUES.as_bytes()[a.span.start - 1], b'*');
    }
}

#[test]
fn anchor_definition_spans_the_value_it_binds() {
    // An inline-anchor refactoring needs the anchored value, not just the label.
    let f = yaml(VALUES);
    let anchor = f
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Anchor && s.name == "base")
        .unwrap();
    let text = anchor.full_span.text(VALUES);
    assert!(text.starts_with("&base"), "got {text:?}");
    assert!(text.contains("retries: 3"), "got {text:?}");
    assert!(text.contains("timeout: 30"), "got {text:?}");
}

#[test]
fn anchor_on_a_scalar_is_found_and_qualified_like_one_on_a_mapping() {
    let f = yaml(VALUES);
    let n = sym(&f, "scalarAnchor::n");
    assert_eq!(n.kind, SymbolKind::Anchor);
    assert_eq!(n.full_span.text(VALUES), "&n 7");
}

#[test]
fn merge_key_becomes_an_alias_reference_and_not_a_key_definition() {
    // `<<` names no field. It splices the aliased mapping in, so the only fact
    // worth recording is the alias it points at.
    let f = yaml(VALUES);
    assert!(
        !f.symbols.iter().any(|s| s.name == "<<"),
        "merge key must not define a symbol: {:?}",
        names(&f)
    );
    let merge = VALUES.find("<<: *base").unwrap();
    let r = f
        .reference_at(merge + 5)
        .expect("merge alias is a reference");
    assert_eq!(r.name, "base");
    assert_eq!(r.kind, ReferenceKind::Identifier);
}

#[test]
fn anchor_names_are_not_also_reported_as_references() {
    let f = yaml("a: &x 1\nb: *x\n");
    let x_refs: Vec<_> = f.references.iter().filter(|r| r.name == "x").collect();
    assert_eq!(x_refs.len(), 1, "got {x_refs:?}");
    assert!(x_refs[0].span.start > 8, "only the alias is a reference");
}

// ---------------------------------------------------------------- structure

#[test]
fn multi_document_streams_keep_documents_apart() {
    let src = "---\nname: a\n---\nname: b\n";
    let f = yaml(src);
    let n: Vec<_> = f.symbols.iter().filter(|s| s.name == "name").collect();
    assert_eq!(n.len(), 2, "got {n:?}");
    assert_ne!(n[0].scope, n[1].scope, "each document is its own scope");
}

#[test]
fn mappings_nest_as_scopes() {
    let f = yaml(VALUES);
    let inner = f.scope_at(VALUES.find("repository:").unwrap()).unwrap();
    let outer = f.scope_at(VALUES.find("image:").unwrap()).unwrap();
    assert_ne!(inner, outer);
    assert!(f.scope_chain(inner).contains(&outer));
}

#[test]
fn every_reference_starts_unresolved() {
    let f = yaml(VALUES);
    assert!(!f.references.is_empty());
    assert!(f
        .references
        .iter()
        .all(|r| r.target.is_none() && r.confidence == Confidence::NameOnly));
}

#[test]
fn block_scalars_contribute_no_spurious_keys() {
    // Text inside a literal block is data, not structure.
    let src = "script: |\n  key: not-a-key\n  echo hi\nafter: 1\n";
    let f = yaml(src);
    assert_eq!(names(&f), vec!["script", "after"], "got {:?}", names(&f));
}

// ------------------------------------------------------------- known gaps

#[test]
fn quoted_keys_report_the_bare_name() {
    // The grammar gives quoted scalars no inner-content node, so @name captures the
    // whole scalar; the extractor trims the quotes, so a quoted key reports the same
    // bare name a plain key does and a rename rewrites only the text inside.
    let f = yaml(VALUES);
    let q = f
        .symbols
        .iter()
        .find(|s| s.name.contains("quoted key"))
        .unwrap_or_else(|| panic!("quoted key missing: {:?}", names(&f)));
    assert_eq!(q.kind, SymbolKind::Key);
    assert_eq!(q.name, "quoted key");
    assert_eq!(q.name_span.text(VALUES), "quoted key");
}

// --------------------------------------------------------------------- Helm

#[test]
fn helm_template_parses_and_extracts_its_structure() {
    let parsed = Parsers::new()
        .parse(Language::Helm, HELM_DEPLOYMENT)
        .unwrap();
    assert!(
        !parsed.has_errors(),
        "masked Helm should parse as YAML: {:?}",
        parsed.error_spans()
    );
    assert_eq!(
        parsed.masked_spans.len(),
        8,
        "every {{{{ }}}} action should be recorded"
    );

    let f = helm(HELM_DEPLOYMENT);
    let got = qualified(&f);
    assert_eq!(
        got,
        vec![
            "apiVersion",
            "kind",
            "metadata",
            "metadata::name",
            "metadata::labels",
            "labels::app",
            "spec",
            "spec::replicas",
            "spec::template",
            "template::spec",
            "spec::containers",
            "containers::name",
            "containers::image",
            "containers::resources",
            "resources::limits",
            "limits::cpu",
        ],
        "got {got:?}"
    );
}

#[test]
fn keys_whose_value_is_entirely_a_template_action_still_extract() {
    // `replicas: {{ .Values.replicaCount }}` masks its action to scalar filler, so the
    // pair has a value and the key spans the whole line. The key must survive, because
    // that is where a values-file cross-reference lands.
    let f = helm(HELM_DEPLOYMENT);
    let replicas = sym(&f, "spec::replicas");
    assert_eq!(replicas.kind, SymbolKind::Key);
    assert_eq!(replicas.name_span.text(HELM_DEPLOYMENT), "replicas");
    assert_eq!(
        replicas.full_span.text(HELM_DEPLOYMENT),
        "replicas: {{ .Values.replicaCount }}"
    );
}

#[test]
fn keys_guarded_by_a_template_conditional_are_still_extracted() {
    // `{{- if }}` / `{{- end }}` mask to blank lines, leaving the guarded keys in
    // place. They are reported unconditionally. This extraction has no notion of
    // a key existing only when a condition holds.
    let f = helm(HELM_DEPLOYMENT);
    assert_eq!(sym(&f, "containers::resources").kind, SymbolKind::Key);
    assert_eq!(
        sym(&f, "limits::cpu").name_span.text(HELM_DEPLOYMENT),
        "cpu"
    );
}

#[test]
fn a_template_action_yields_no_yaml_facts_of_its_own() {
    // Template contents are masked before parsing, so `include`, `if` and
    // `chart.fullname` are invisible to the YAML query by construction. Masking must
    // never invent a key: a symbol overlapping an action would mean the structure of
    // the document was read out of filler.
    let f = helm(HELM_DEPLOYMENT);
    let leaked: Vec<_> = f
        .symbols
        .iter()
        .map(|s| (s.name.clone(), s.name_span))
        .filter(|(name, _)| {
            [
                "include",
                "Values",
                "appName",
                "replicaCount",
                "fullname",
                "if",
                "end",
            ]
            .contains(&name.as_str())
        })
        .collect();
    assert!(leaked.is_empty(), "template contents leaked: {leaked:?}");

    let parsed = Parsers::new()
        .parse(Language::Helm, HELM_DEPLOYMENT)
        .unwrap();
    for action in &parsed.masked_spans {
        for s in &f.symbols {
            assert!(
                s.name_span.end <= action.start || s.name_span.start >= action.end,
                "symbol {:?} overlaps masked action {:?}",
                s.name,
                action.text(HELM_DEPLOYMENT)
            );
        }
    }
}

#[test]
fn the_values_a_template_reads_are_the_only_facts_inside_an_action() {
    // The one exception, and it is deliberate: `{{ .Values.image.tag }}` is a use of
    // a key the values file declares, and a rename of that key has to rewrite it. The
    // paths come from parsing the actions themselves, not from the masked text, so
    // each reference spans the final segment, `tag`, not `.Values.image.tag`.
    let f = helm(HELM_DEPLOYMENT);
    let parsed = Parsers::new()
        .parse(Language::Helm, HELM_DEPLOYMENT)
        .unwrap();

    assert!(
        !f.references.is_empty(),
        "a template that reads .Values has references"
    );
    for r in &f.references {
        assert_eq!(
            r.kind,
            ReferenceKind::StringRef,
            "a values path is string-keyed, like a CSS class: {r:?}"
        );
        let inside = parsed
            .masked_spans
            .iter()
            .any(|a| r.span.start >= a.start && r.span.end <= a.end);
        assert!(inside, "reference {:?} is not inside any action", r.name);
        assert_eq!(
            r.span.text(HELM_DEPLOYMENT),
            r.name,
            "the span is the key alone, so a rename rewrites only it"
        );
        assert!(
            !r.name.contains('.') && !r.name.contains(' '),
            "the whole chain leaked into the name: {:?}",
            r.name
        );
    }
}

#[test]
fn helm_names_never_contain_masking_whitespace() {
    // Masked bytes are real spaces in the parsed text; a name that ran into one
    // would produce an edit with trailing blanks.
    let f = helm(HELM_DEPLOYMENT);
    for s in &f.symbols {
        assert_eq!(s.name.trim(), s.name, "padded name {:?}", s.name);
        assert!(!s.name.is_empty(), "empty name at {:?}", s.name_span);
    }
}

#[test]
fn helm_anchors_work_the_same_as_in_plain_yaml() {
    let src = "common: &c\n  a: {{ .Values.a }}\nuse:\n  <<: *c\n";
    let f = helm(src);
    let anchor = f
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Anchor)
        .unwrap_or_else(|| panic!("no anchor: {:?}", names(&f)));
    assert_eq!(anchor.name, "c");
    let alias: Vec<_> = f.references.iter().filter(|r| r.name == "c").collect();
    assert_eq!(alias.len(), 1, "got {alias:?}");
}

#[test]
fn a_template_action_in_key_position_breaks_the_parse() {
    // Known gap, recorded and not papered over: masking `{{ .Values.k }}: v`
    // leaves `                : v`, and a mapping entry with a blank key is not
    // valid YAML, so the document carries a parse error. Extraction still runs
    // and still finds the surrounding keys, but the masked line is lost.
    let src = "before: 1\n{{ .Values.k }}: v\nafter: 2\n";
    let parsed = Parsers::new().parse(Language::Helm, src).unwrap();
    assert!(
        parsed.has_errors(),
        "a templated key is expected to be visible as a parse error"
    );
    let f = helm(src);
    assert!(names(&f).contains(&"before"), "got {:?}", names(&f));
    assert!(
        !names(&f).iter().any(|n| n.contains('{')),
        "no fragment of the action becomes a name: {:?}",
        names(&f)
    );
}
