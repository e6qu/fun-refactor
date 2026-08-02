//! Helm values precedence when the caller supplies the command line.
//!
//! A workspace scan sees a chart, its subcharts and whatever `values-*.yaml` files
//! sit beside them. It cannot see the invocation: whether a values file is passed
//! with `-f` at all, in which order two of them were written, or that a `--set`
//! overrides both. That is not an inherent limit, it is a missing input — so these
//! tests supply it, and pin both halves: what the same query answers with nothing
//! supplied (unchanged), and what it answers when told.
//!
//! Every assertion below is of what the code does, including where it still
//! refuses: an answer from a partial command line says it is partial.

use fun_refactor::{
    analysis::provenance::{
        self, consumers_with_inputs, provenance, provenance_with_inputs, StopReason, ValuesInputs,
    },
    helm,
    index::Index,
    model::{Symbol, SymbolId, SymbolKind},
    scan::{scan, ScanOptions},
};
use std::path::PathBuf;

// ------------------------------------------------------------------ fixtures

const PARENT_VALUES: &str = r#"replicaCount: 1
image:
  tag: "1.0"
  repository: parent
extra-args: "--quiet"
mysql:
  image:
    tag: "8.0"
"#;

const SUBCHART_VALUES: &str = r#"image:
  tag: "5.7"
  repository: mysql
"#;

const STAGE_VALUES: &str = r#"replicaCount: 2
mysql:
  image:
    tag: "8.3"
"#;

const PROD_VALUES: &str = r#"replicaCount: 3
mysql:
  image:
    tag: "8.4"
"#;

const DEPLOYMENT: &str = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ .Release.Name }}
spec:
  replicas: {{ .Values.replicaCount }}
  template:
    spec:
      containers:
        - image: "{{ .Values.image.repository }}:{{ .Values.image.tag }}"
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          args: {{ index .Values "extra-args" }}
"#;

/// A parent chart, one subchart, and two values files someone may or may not pass.
fn chart() -> (tempfile::TempDir, Index) {
    workspace(&[
        ("app/Chart.yaml", "name: app\nversion: 0.1.0\n"),
        ("app/values.yaml", PARENT_VALUES),
        ("app/values-stage.yaml", STAGE_VALUES),
        ("app/values-prod.yaml", PROD_VALUES),
        (
            "app/charts/mysql/Chart.yaml",
            "name: mysql\nversion: 8.0.0\n",
        ),
        ("app/charts/mysql/values.yaml", SUBCHART_VALUES),
        ("app/templates/deployment.yaml", DEPLOYMENT),
    ])
}

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();
    (tmp, index)
}

/// The key whose dotted path is `path`, in the file whose path ends with `file`.
fn key_with_path(index: &Index, file: &str, path: &str) -> SymbolId {
    let wanted: Vec<&str> = path.split('.').collect();
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
        .unwrap_or_else(|| panic!("no key {path} in {file}"))
        .id
}

fn tag_competition(result: &provenance::Provenance) -> &provenance::Competition {
    result
        .competitions
        .iter()
        .find(|c| c.subject.contains("image.tag"))
        .unwrap_or_else(|| panic!("no image.tag competition: {:?}", result.competitions))
}

/// `(precedence label, value text, wins)` for every source, strongest first.
fn listing(competition: &provenance::Competition) -> Vec<(String, String, bool)> {
    competition
        .sources
        .iter()
        .map(|s| (s.precedence.label.clone(), s.hop.text.clone(), s.wins))
        .collect()
}

fn values_file(tmp: &tempfile::TempDir, name: &str) -> PathBuf {
    tmp.path().join("app").join(name)
}

fn stops(result: &provenance::Provenance) -> Vec<String> {
    result.stops.iter().map(|(_, r)| r.to_string()).collect()
}

// ------------------------------------------------- nothing supplied: unchanged

#[test]
fn with_no_inputs_the_command_line_still_decides_nothing() {
    // The baseline this whole feature is additive to: two values files beside the
    // chart both set the key, and which one applies is the invocation's business.
    let (_tmp, index) = chart();
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = provenance(&index, tag, 5).unwrap();

    let competition = tag_competition(&result);
    assert!(!competition.decided);
    assert!(
        competition.winner().is_none(),
        "two files that may be passed in either order have no winner: {:?}",
        listing(competition)
    );
    assert_eq!(
        listing(competition)
            .iter()
            .map(|(label, _, _)| label.clone())
            .collect::<Vec<_>>(),
        vec![
            // Same rank, so the display order is the file name reversed: neither
            // outranks the other, which is exactly what leaves this undecided.
            "user-supplied -f values-stage.yaml".to_string(),
            "user-supplied -f values-prod.yaml".to_string(),
            "parent chart values (app)".to_string(),
            "chart defaults (mysql)".to_string(),
        ]
    );
    assert!(result.stopped_because(
        |r| matches!(r, StopReason::PrecedenceUndetermined(m) if m.contains("comes last"))
    ));
    // Nothing supplied means nothing decided *given* anything.
    assert!(!result.stopped_because(|r| matches!(r, StopReason::DecidedGivenInputs { .. })));
}

#[test]
fn passing_no_inputs_explicitly_is_the_same_answer_as_passing_none() {
    let (_tmp, index) = chart();
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let plain = provenance(&index, tag, 5).unwrap();
    let empty = provenance_with_inputs(&index, tag, 5, &ValuesInputs::default()).unwrap();
    assert_eq!(stops(&plain), stops(&empty));
    assert_eq!(
        listing(tag_competition(&plain)),
        listing(tag_competition(&empty))
    );
}

// ----------------------------------------------------------- one `-f` decides

#[test]
fn one_values_file_decides_the_winner_and_keeps_every_loser() {
    let (tmp, index) = chart();
    let inputs = ValuesInputs {
        files: vec![values_file(&tmp, "values-prod.yaml")],
        sets: Vec::new(),
    };
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = provenance_with_inputs(&index, tag, 5, &inputs).unwrap();

    let competition = tag_competition(&result);
    assert!(competition.decided, "the command line was described");
    assert_eq!(
        listing(competition),
        vec![
            (
                "user-supplied -f values-prod.yaml (1 of 1)".to_string(),
                "tag: \"8.4\"".to_string(),
                true
            ),
            (
                "parent chart values (app)".to_string(),
                "tag: \"8.0\"".to_string(),
                false
            ),
            (
                "chart defaults (mysql)".to_string(),
                "tag: \"5.7\"".to_string(),
                false
            ),
            // Listed, not competing: the caller said this file is not passed.
            (
                "-f values-stage.yaml (not passed)".to_string(),
                "tag: \"8.3\"".to_string(),
                false
            ),
        ]
    );
    assert_eq!(competition.losers().len(), 3, "nothing is dropped");
    assert!(competition
        .sources
        .iter()
        .find(|s| s.precedence.label.contains("not passed"))
        .unwrap()
        .reason
        .contains("sets nothing in this invocation"));
    // Each surviving source still points at its own file and line.
    for source in competition.sources.iter().filter(|s| s.hop.line > 0) {
        assert!(source.hop.file.exists());
    }
    assert!(
        !result.stopped_because(|r| matches!(r, StopReason::PrecedenceUndetermined(_))),
        "the inputs settle it: {:?}",
        stops(&result)
    );
}

#[test]
fn a_values_file_that_sets_nothing_for_the_key_leaves_the_chart_value_standing() {
    let (tmp, index) = chart();
    // values-prod.yaml sets no `image.repository`.
    let inputs = ValuesInputs {
        files: vec![values_file(&tmp, "values-prod.yaml")],
        sets: Vec::new(),
    };
    let repository = key_with_path(&index, "charts/mysql/values.yaml", "image.repository");
    let result = provenance_with_inputs(&index, repository, 5, &inputs).unwrap();

    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::DecidedGivenInputs { decided_by, .. } if decided_by.contains("chart defaults")
        )),
        "got {:?}",
        stops(&result)
    );
}

#[test]
fn a_key_only_a_file_that_is_not_passed_sets_is_unset_in_this_invocation() {
    // The other half of "which file is passed": a key whose only source is a file
    // the caller did not list is not a competition anyone wins — it is unset, and
    // the file that would supply it is named.
    let (tmp, index) = workspace(&[
        ("app/Chart.yaml", "name: app\nversion: 0.1.0\n"),
        ("app/values.yaml", "replicaCount: 1\n"),
        ("app/values-prod.yaml", "replicaCount: 3\n"),
        ("app/values-extra.yaml", "onlyHere: yes\n"),
    ]);
    let inputs = ValuesInputs {
        files: vec![values_file(&tmp, "values-prod.yaml")],
        sets: Vec::new(),
    };
    let only_here = key_with_path(&index, "app/values-extra.yaml", "onlyHere");
    let result = provenance_with_inputs(&index, only_here, 5, &inputs).unwrap();

    assert!(
        result.competitions.is_empty(),
        "nothing competes for a key nothing supplies: {:?}",
        result.competitions
    );
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { required: true, sources, .. }
                if sources.contains("`-f values-extra.yaml`") && sources.contains("do not pass it")
        )),
        "got {:?}",
        stops(&result)
    );
}

// ------------------------------------------------------- order of several `-f`

#[test]
fn two_values_files_apply_in_the_order_they_were_given() {
    let (tmp, index) = chart();
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");

    let stage_then_prod = ValuesInputs {
        files: vec![
            values_file(&tmp, "values-stage.yaml"),
            values_file(&tmp, "values-prod.yaml"),
        ],
        sets: Vec::new(),
    };
    let result = provenance_with_inputs(&index, tag, 5, &stage_then_prod).unwrap();
    let competition = tag_competition(&result);
    assert!(competition.decided);
    let winner = competition.winner().unwrap();
    assert_eq!(winner.hop.text, "tag: \"8.4\"");
    assert_eq!(
        winner.precedence.label,
        "user-supplied -f values-prod.yaml (2 of 2)"
    );
    assert!(winner.reason.contains("the last `-f` supplied"));

    // The same two files, written the other way round, give the other answer.
    let prod_then_stage = ValuesInputs {
        files: vec![
            values_file(&tmp, "values-prod.yaml"),
            values_file(&tmp, "values-stage.yaml"),
        ],
        sets: Vec::new(),
    };
    let result = provenance_with_inputs(&index, tag, 5, &prod_then_stage).unwrap();
    let competition = tag_competition(&result);
    assert!(competition.decided);
    assert_eq!(competition.winner().unwrap().hop.text, "tag: \"8.3\"");
    // Both are ranked above the chart, and neither is dropped.
    assert_eq!(competition.sources.len(), 4);
}

// ------------------------------------------------------------- `--set` is top

#[test]
fn a_set_beats_every_values_file_however_many_were_passed() {
    let (tmp, index) = chart();
    let inputs = ValuesInputs {
        files: vec![
            values_file(&tmp, "values-stage.yaml"),
            values_file(&tmp, "values-prod.yaml"),
        ],
        sets: helm::parse_set("mysql.image.tag=9.9", false).unwrap(),
    };
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = provenance_with_inputs(&index, tag, 5, &inputs).unwrap();

    let competition = tag_competition(&result);
    assert!(competition.decided);
    let winner = competition.winner().unwrap();
    assert_eq!(winner.hop.text, "--set mysql.image.tag=9.9");
    assert_eq!(winner.precedence.label, "--set on the command line");
    assert!(winner.reason.contains("outranks every values file"));
    // A command-line source has no file to point at, and says so with line 0.
    assert_eq!(winner.hop.line, 0);
    assert_eq!(
        winner.hop.file,
        std::path::PathBuf::from(provenance::COMMAND_LINE)
    );
    // Both files and both chart levels are still listed under it.
    assert_eq!(competition.losers().len(), 4, "{:?}", listing(competition));
    assert!(competition.losers().iter().all(|s| s
        .reason
        .contains("overridden by --set on the command line")
        || s.reason.contains("sets nothing")));
}

#[test]
fn the_last_set_of_a_repeated_key_is_the_one_that_applies() {
    let (_tmp, index) = chart();
    let mut sets = helm::parse_set("replicaCount=5", false).unwrap();
    sets.extend(helm::parse_set("replicaCount=7", false).unwrap());
    let inputs = ValuesInputs {
        files: Vec::new(),
        sets,
    };
    let replicas = key_with_path(&index, "app/values.yaml", "replicaCount");
    let result = provenance_with_inputs(&index, replicas, 5, &inputs).unwrap();
    let competition = &result.competitions[0];
    assert_eq!(
        competition.winner().unwrap().hop.text,
        "--set replicaCount=7"
    );
}

// ------------------------------------------------- a key no values file has

#[test]
fn a_set_for_a_key_no_values_file_declares_is_reported_as_introducing_it() {
    // `.Values.image.pullPolicy` is read by the template and declared nowhere.
    let (_tmp, index) = chart();
    let inputs = ValuesInputs {
        files: Vec::new(),
        sets: helm::parse_set("image.pullPolicy=Always", false).unwrap(),
    };
    let policy = key_with_path(
        &index,
        "templates/deployment.yaml",
        "spec.template.spec.containers.imagePullPolicy",
    );
    let result = provenance_with_inputs(&index, policy, 6, &inputs).unwrap();

    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::Origin(o)
                if o.contains("introduces .Values.image.pullPolicy")
                    && o.contains("no values file")
        )),
        "got {:?}",
        stops(&result)
    );
    let competition = result
        .competitions
        .iter()
        .find(|c| c.subject.contains("pullPolicy"))
        .unwrap_or_else(|| panic!("no competition: {:?}", result.competitions));
    assert!(competition
        .subject
        .contains("introduced by the inputs supplied"));
    assert_eq!(
        competition.winner().unwrap().hop.text,
        "--set image.pullPolicy=Always"
    );
}

#[test]
fn without_that_set_the_same_key_is_still_simply_unset() {
    let (_tmp, index) = chart();
    let policy = key_with_path(
        &index,
        "templates/deployment.yaml",
        "spec.template.spec.containers.imagePullPolicy",
    );
    let result = provenance(&index, policy, 6).unwrap();
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { name, required: true, sources }
                if name.contains("image.pullPolicy") && sources.contains("--set")
        )),
        "got {:?}",
        stops(&result)
    );
}

#[test]
fn an_input_that_sets_nothing_relevant_says_so_rather_than_going_quiet() {
    let (_tmp, index) = chart();
    let inputs = ValuesInputs {
        files: Vec::new(),
        sets: helm::parse_set("somethingElse=1", false).unwrap(),
    };
    let policy = key_with_path(
        &index,
        "templates/deployment.yaml",
        "spec.template.spec.containers.imagePullPolicy",
    );
    let result = provenance_with_inputs(&index, policy, 6, &inputs).unwrap();
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { sources, .. }
                if sources.contains("the inputs supplied") && sources.contains("set none")
        )),
        "got {:?}",
        stops(&result)
    );
}

// ------------------------------------------------- honesty about what is left

#[test]
fn a_partial_command_line_decides_only_given_what_was_supplied() {
    let (_tmp, index) = chart();
    let inputs = ValuesInputs {
        files: Vec::new(),
        sets: helm::parse_set("mysql.image.tag=9.9", false).unwrap(),
    };
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = provenance_with_inputs(&index, tag, 5, &inputs).unwrap();

    let stop = stops(&result)
        .into_iter()
        .find(|s| s.contains("decided given the inputs supplied"))
        .unwrap_or_else(|| panic!("no partial-input stop: {:?}", stops(&result)));
    assert!(stop.contains("--set mysql.image.tag=9.9"), "{stop}");
    assert!(
        stop.contains("a `-f` values file"),
        "the channel never described is named: {stop}"
    );
    assert!(
        !stop.contains("a `--set`"),
        "the channel that was described is not: {stop}"
    );
}

#[test]
fn a_command_line_described_in_full_carries_no_caveat() {
    let (tmp, index) = chart();
    let inputs = ValuesInputs {
        files: vec![values_file(&tmp, "values-prod.yaml")],
        sets: helm::parse_set("mysql.image.tag=9.9", false).unwrap(),
    };
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = provenance_with_inputs(&index, tag, 5, &inputs).unwrap();

    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::DecidedGivenInputs { unsupplied, decided_by, .. }
                if unsupplied.is_empty() && decided_by.contains("--set")
        )),
        "got {:?}",
        stops(&result)
    );
    assert!(stops(&result)
        .iter()
        .any(|s| s.contains("the strongest of the inputs supplied")));
    assert!(
        !stops(&result)
            .iter()
            .any(|s| s.contains("decided given the inputs supplied")),
        "nothing is left open: {:?}",
        stops(&result)
    );
}

#[test]
fn the_forward_direction_takes_the_same_inputs() {
    let (tmp, index) = chart();
    let inputs = ValuesInputs {
        files: vec![values_file(&tmp, "values-prod.yaml")],
        sets: Vec::new(),
    };
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = consumers_with_inputs(&index, tag, 5, &inputs).unwrap();
    let competition = tag_competition(&result);
    assert!(competition.decided);
    assert_eq!(competition.winner().unwrap().hop.text, "tag: \"8.4\"");
}

// --------------------------------------------------------- resolving the flags

#[test]
fn a_values_file_outside_the_scan_is_refused_rather_than_ignored() {
    let (_tmp, index) = chart();
    let inputs = ValuesInputs {
        files: vec![PathBuf::from("nowhere/values-x.yaml")],
        sets: Vec::new(),
    };
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let error = provenance_with_inputs(&index, tag, 5, &inputs)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not in the scanned workspace"), "{error}");
}

#[test]
fn a_values_file_names_the_scanned_file_by_a_relative_path() {
    let (_tmp, index) = chart();
    let inputs = ValuesInputs {
        files: vec![PathBuf::from("app/values-prod.yaml")],
        sets: Vec::new(),
    };
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = provenance_with_inputs(&index, tag, 5, &inputs).unwrap();
    assert_eq!(
        tag_competition(&result).winner().unwrap().hop.text,
        "tag: \"8.4\""
    );
}

// --------------------------------------------------------- Helm's --set syntax

#[test]
fn set_paths_follow_helms_own_syntax() {
    let dotted = helm::parse_set("image.tag=1.2", false).unwrap();
    assert_eq!(dotted.len(), 1);
    assert_eq!(dotted[0].keys(), vec!["image", "tag"]);
    assert_eq!(dotted[0].value, "1.2");
    assert!(!dotted[0].string);

    // A list index addresses an element; the values index records mapping keys
    // only, so the index is kept in the text and dropped from the key path.
    let indexed = helm::parse_set("ports[0].name=http", false).unwrap();
    assert_eq!(
        indexed[0].path,
        vec![
            helm::SetSegment::Key("ports".into()),
            helm::SetSegment::Index(0),
            helm::SetSegment::Key("name".into()),
        ]
    );
    assert_eq!(indexed[0].keys(), vec!["ports", "name"]);

    // Commas separate several assignments in one flag, as helm does.
    let several = helm::parse_set("a=1,b.c=2", false).unwrap();
    assert_eq!(several.len(), 2);
    assert_eq!(several[1].keys(), vec!["b", "c"]);
    assert_eq!(several[1].text, "b.c=2");

    // An escaped dot is part of the key, not a separator.
    let escaped = helm::parse_set(r"annotations.example\.com/team=infra", false).unwrap();
    assert_eq!(escaped[0].keys(), vec!["annotations", "example.com/team"]);

    // A value may hold `=` and commas of its own once escaped.
    let value = helm::parse_set(r"args=--a\,--b", false).unwrap();
    assert_eq!(value[0].value, "--a,--b");

    let string = helm::parse_set("tag=1.20", true).unwrap();
    assert!(string[0].string);
    assert_eq!(string[0].flag(), "--set-string");
    assert_eq!(string[0].describe(), "`--set-string tag=1.20`");
}

#[test]
fn unsupported_set_syntax_is_refused_by_name() {
    let list = helm::parse_set("a={x,y}", false).unwrap_err().to_string();
    assert!(list.contains("list syntax"), "{list}");
    assert!(
        list.contains("-f"),
        "the refusal names what does work: {list}"
    );

    let bare = helm::parse_set("justakey", false).unwrap_err().to_string();
    assert!(bare.contains("not an assignment"), "{bare}");

    let empty = helm::parse_set("=1", false).unwrap_err().to_string();
    assert!(empty.contains("empty key"), "{empty}");

    let index = helm::parse_set("a[x]=1", false).unwrap_err().to_string();
    assert!(index.contains("not a list index"), "{index}");
}

#[test]
fn an_order_between_set_and_set_string_that_the_flags_lost_is_refused() {
    // Helm applies --set and --set-string in the order they were written; two flag
    // lists cannot say what that was, so the one case where it matters is refused.
    let clash = ValuesInputs::parse(
        &[],
        &["image.tag=1".to_string()],
        &["image.tag=2".to_string()],
    )
    .unwrap_err()
    .to_string();
    assert!(clash.contains("order they were written"), "{clash}");

    // Different keys never need that order, and are accepted.
    let fine = ValuesInputs::parse(
        &[],
        &["image.tag=1".to_string()],
        &["image.repository=repo".to_string()],
    )
    .unwrap();
    assert_eq!(fine.sets.len(), 2);
    assert!(fine.unsupplied() == vec!["a `-f` values file".to_string()]);
}

#[test]
fn parsed_inputs_describe_themselves_as_a_command_line() {
    let inputs = ValuesInputs::parse(
        &[PathBuf::from("values-prod.yaml")],
        &["a.b=c".to_string()],
        &[],
    )
    .unwrap();
    assert_eq!(inputs.describe(), "-f values-prod.yaml --set a.b=c");
    assert!(!inputs.is_empty());
    assert!(ValuesInputs::default().is_empty());
}

// ------------------------------------------------- `index .Values "literal"`

#[test]
fn index_with_a_literal_key_resolves_to_that_values_key() {
    // The idiom for a key that is not a valid identifier. It was a documented
    // residual of the masked-action work; the string arguments are the path.
    assert_eq!(
        helm::values_paths_in(r#"{{ index .Values "extra-args" }}"#),
        vec![vec!["extra-args".to_string()]]
    );
    assert_eq!(
        helm::values_paths_in(r#"{{ index .Values "a-b" "c-d" | quote }}"#),
        vec![vec!["a-b".to_string(), "c-d".to_string()]]
    );
    // A base that already has a path keeps it, with the literal appended.
    assert_eq!(
        helm::values_paths_in(r#"{{ index .Values.image "pull-policy" }}"#),
        vec![vec!["image".to_string(), "pull-policy".to_string()]]
    );
    // `$` reaches the root context the same way.
    assert_eq!(
        helm::values_paths_in(r#"{{ index $.Values "extra-args" }}"#),
        vec![vec!["extra-args".to_string()]]
    );
}

#[test]
fn a_computed_index_key_is_named_unresolved_rather_than_invented() {
    let computed = helm::parse_action(r#"{{ index .Values $key }}"#);
    assert!(computed.values_paths().is_empty());
    assert!(
        computed
            .problems
            .iter()
            .any(|p| p.contains("computes its key")),
        "{:?}",
        computed.problems
    );

    let nested = helm::parse_action(r#"{{ index (index .Values "a") "b" }}"#);
    assert!(
        nested
            .problems
            .iter()
            .any(|p| p.contains("parenthesised expression")),
        "{:?}",
        nested.problems
    );
}

#[test]
fn a_template_reaching_a_hyphenated_key_now_lands_on_the_values_entry() {
    let (_tmp, index) = chart();
    let args = key_with_path(
        &index,
        "templates/deployment.yaml",
        "spec.template.spec.containers.args",
    );
    let result = provenance(&index, args, 6).unwrap();
    assert!(
        result
            .hops
            .iter()
            .any(|h| h.text.contains("extra-args: \"--quiet\"")),
        "the walk should reach the values key: {:?}",
        result
            .hops
            .iter()
            .map(|h| h.text.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_hyphenated_key_reached_by_index_takes_the_supplied_inputs_too() {
    let (_tmp, index) = chart();
    let inputs = ValuesInputs {
        files: Vec::new(),
        sets: helm::parse_set(r"extra-args=--verbose", false).unwrap(),
    };
    let args = key_with_path(
        &index,
        "templates/deployment.yaml",
        "spec.template.spec.containers.args",
    );
    let result = provenance_with_inputs(&index, args, 6, &inputs).unwrap();
    let competition = result
        .competitions
        .iter()
        .find(|c| c.subject.contains("extra-args"))
        .unwrap_or_else(|| panic!("no competition: {:?}", result.competitions));
    assert_eq!(
        competition.winner().unwrap().hop.text,
        "--set extra-args=--verbose"
    );
}
