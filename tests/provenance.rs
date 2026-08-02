//! Config-language value provenance, exercised through the public API.
//!
//! These languages are messy in different ways, and the point of the analysis is
//! that it says so: every test below either pins an exact substitution/override
//! answer, or pins the *honest* refusal to answer — an external input, a masked
//! template action, an undecidable precedence — rather than a guess.

use fun_refactor::{
    analysis::provenance::{consumers, provenance, specificity, EdgeKind, StopReason},
    analysis::{flow, provenance as prov},
    index::Index,
    model::{Symbol, SymbolId, SymbolKind},
    scan::{scan, ScanOptions},
};

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

/// The one symbol with this name in this file.
fn symbol_in<'a>(index: &'a Index, file: &str, name: &str) -> &'a Symbol {
    let mut hits: Vec<&Symbol> = index
        .symbols
        .iter()
        .filter(|s| s.name == name && s.file.to_string_lossy().ends_with(file))
        .collect();
    assert!(!hits.is_empty(), "no symbol {name} in {file}");
    hits.sort_by_key(|s| s.full_span.start);
    hits[0]
}

fn id_in(index: &Index, file: &str, name: &str) -> SymbolId {
    symbol_in(index, file, name).id
}

fn hop_texts(p: &prov::Provenance) -> Vec<String> {
    p.hops.iter().map(|h| h.text.clone()).collect()
}

fn has_hop(p: &prov::Provenance, needle: &str) -> bool {
    p.hops.iter().any(|h| h.text.contains(needle))
}

// ============================================================== Terraform/HCL

const VARIABLES_TF: &str = r#"variable "region" {
  type    = string
  default = "us-east-1"
}

variable "env" {
  type = string
}

variable "unset" {
  type = string
}
"#;

const MAIN_TF: &str = r#"locals {
  prefix = "${var.env}-${var.region}"
  name   = "${local.prefix}-app"
}

resource "aws_s3_bucket" "main" {
  bucket = local.name
}

output "arn" {
  value = aws_s3_bucket.main.arn
}

module "network" {
  source = "./modules/network"
  cidr   = "10.0.0.0/16"
}

output "vpc" {
  value = module.network.vpc_id
}

output "missing" {
  value = module.network.nope
}
"#;

const MODULE_TF: &str = r#"variable "cidr" {
  type    = string
  default = "10.0.0.0/8"
}

output "vpc_id" {
  value = "vpc-static"
}
"#;

fn terraform() -> (tempfile::TempDir, Index) {
    workspace(&[
        ("infra/variables.tf", VARIABLES_TF),
        ("infra/main.tf", MAIN_TF),
        ("infra/modules/network/main.tf", MODULE_TF),
        ("infra/terraform.tfvars", "env = \"staging\"\n"),
        ("infra/zz.auto.tfvars", "env = \"prod\"\n"),
    ])
}

#[test]
fn a_local_keeps_every_hop_of_its_substitution_chain() {
    // local.name ← local.prefix ← var.env / var.region. Checkov's mistake is to
    // substitute in place; every intermediate expression must survive here.
    let (_tmp, index) = terraform();
    let name = id_in(&index, "main.tf", "name");
    let result = provenance(&index, name, 10).unwrap();

    let texts = hop_texts(&result);
    assert!(
        texts.iter().any(|t| t.starts_with("local.name = ")),
        "{texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|t| t.contains("local.prefix = \"${var.env}-${var.region}\"")),
        "the intermediate expression must be retained verbatim: {texts:?}"
    );
    assert!(texts.iter().any(|t| t.starts_with("var.env")), "{texts:?}");
    assert!(
        texts.iter().any(|t| t.starts_with("var.region")),
        "{texts:?}"
    );

    // Hops nest: each substitution is one level deeper than what it fed.
    let depth_of = |needle: &str| {
        result
            .hops
            .iter()
            .find(|h| h.text.starts_with(needle))
            .map(|h| h.depth)
            .unwrap_or_else(|| panic!("no hop {needle}"))
    };
    assert_eq!(depth_of("local.name ="), 0);
    assert_eq!(depth_of("local.prefix ="), 1);
    assert_eq!(depth_of("var.env"), 2);

    // Every hop carries its own file and a real line number.
    for hop in &result.hops {
        assert!(hop.line >= 1, "hop without a line: {hop:?}");
        assert!(hop.file.exists(), "hop without a file: {hop:?}");
    }
    assert!(result.hops.iter().any(|h| h.kind == EdgeKind::Substitution));
}

#[test]
fn a_variable_with_no_default_is_an_external_input() {
    let (_tmp, index) = terraform();
    let unset = id_in(&index, "variables.tf", "unset");
    let result = provenance(&index, unset, 5).unwrap();

    let external: Vec<&StopReason> = result
        .stops
        .iter()
        .map(|(_, r)| r)
        .filter(|r| matches!(r, StopReason::ExternalInput { .. }))
        .collect();
    assert_eq!(external.len(), 1, "got {:?}", result.stops);
    match external[0] {
        StopReason::ExternalInput {
            name,
            required,
            sources,
        } => {
            assert_eq!(name, "var.unset");
            assert!(*required, "nothing in the workspace sets it");
            assert!(sources.contains("tfvars"), "{sources}");
            assert!(sources.contains("TF_VAR_"), "{sources}");
        }
        other => panic!("{other:?}"),
    }
    // Nothing was invented in its place.
    assert!(result.competitions.is_empty(), "{:?}", result.competitions);
}

#[test]
fn a_variable_with_a_default_is_still_externally_overridable() {
    let (_tmp, index) = terraform();
    let region = id_in(&index, "variables.tf", "region");
    let result = provenance(&index, region, 5).unwrap();

    assert!(result.stopped_because(|r| matches!(
        r,
        StopReason::ExternalInput {
            required: false,
            ..
        }
    )));
    let competition = &result.competitions[0];
    assert_eq!(competition.sources.len(), 1);
    let winner = competition.winner().unwrap();
    assert!(winner.hop.text.contains("us-east-1"));
    assert_eq!(
        winner.hop.kind,
        EdgeKind::Default,
        "a default is a fallback, not an override"
    );
    assert!(
        !competition.decided,
        "-var on the CLI is invisible from the workspace, so nothing is final"
    );
}

#[test]
fn competing_tfvars_files_are_all_reported_with_the_winner_marked() {
    // Terraform's order: default < terraform.tfvars < *.auto.tfvars. All three set
    // `env`; the losers must stay visible.
    let (_tmp, index) = terraform();
    let env = id_in(&index, "variables.tf", "env");
    let result = provenance(&index, env, 5).unwrap();

    let competition = result
        .competitions
        .iter()
        .find(|c| c.subject.contains("var.env"))
        .expect("a competition for var.env");
    assert!(
        competition.model.contains("terraform.tfvars"),
        "{}",
        competition.model
    );

    let labels: Vec<&str> = competition
        .sources
        .iter()
        .map(|s| s.precedence.label.as_str())
        .collect();
    assert_eq!(
        labels,
        vec!["zz.auto.tfvars (auto-loaded)", "terraform.tfvars"],
        "strongest first"
    );

    let winner = competition.winner().expect("a winner");
    assert!(winner.hop.text.contains("prod"), "{}", winner.hop.text);
    assert_eq!(winner.precedence.rank, 3);

    let losers = competition.losers();
    assert_eq!(losers.len(), 1);
    assert!(losers[0].hop.text.contains("staging"));
    assert!(
        losers[0].reason.contains("overridden by"),
        "a loser must say who beat it: {}",
        losers[0].reason
    );
    // The losing file and line stay addressable.
    assert!(losers[0].hop.file.ends_with("terraform.tfvars"));
    assert_eq!(losers[0].hop.line, 1);
}

#[test]
fn a_resource_attribute_is_reported_as_computed_at_apply_time() {
    let (_tmp, index) = terraform();
    let arn = id_in(&index, "main.tf", "arn");
    let result = provenance(&index, arn, 5).unwrap();
    assert!(
        result.stopped_because(
            |r| matches!(r, StopReason::ComputedAtApply(what) if what == "aws_s3_bucket.main.arn")
        ),
        "got {:?}",
        result.stops
    );
}

#[test]
fn a_module_output_is_followed_into_the_child_module() {
    let (_tmp, index) = terraform();
    let vpc = id_in(&index, "main.tf", "vpc");
    let result = provenance(&index, vpc, 5).unwrap();

    assert!(
        has_hop(&result, "output.vpc_id = \"vpc-static\""),
        "{:?}",
        hop_texts(&result)
    );
    assert!(result
        .hops
        .iter()
        .any(|h| h.kind == EdgeKind::ModuleOutput && h.file.ends_with("modules/network/main.tf")));
}

#[test]
fn a_module_output_that_does_not_exist_is_reported_not_invented() {
    let (_tmp, index) = terraform();
    let missing = id_in(&index, "main.tf", "missing");
    let result = provenance(&index, missing, 5).unwrap();
    assert!(
        result.stopped_because(
            |r| matches!(r, StopReason::Unresolved(what) if what.contains("nope"))
        ),
        "got {:?}",
        result.stops
    );
}

#[test]
fn the_depth_limit_is_reported_rather_than_silently_truncating() {
    let (_tmp, index) = terraform();
    let name = id_in(&index, "main.tf", "name");
    let result = provenance(&index, name, 1).unwrap();
    assert!(
        result
            .stops
            .iter()
            .any(|(_, r)| *r == StopReason::DepthLimit),
        "got {:?}",
        result.stops
    );
    assert!(
        result.hops.iter().all(|h| h.depth <= 1),
        "the walk really did stop: {:?}",
        hop_texts(&result)
    );
}

#[test]
fn a_cycle_in_locals_terminates() {
    let (_tmp, index) = workspace(&[(
        "infra/main.tf",
        "locals {\n  a = local.b\n  b = local.a\n}\n",
    )]);
    let a = id_in(&index, "main.tf", "a");
    let result = provenance(&index, a, 50).unwrap();
    assert!(result.hops.len() < 10, "{:?}", hop_texts(&result));
}

#[test]
fn an_unresolvable_variable_reference_is_reported() {
    let (_tmp, index) = workspace(&[("infra/main.tf", "locals {\n  a = var.nowhere\n}\n")]);
    let a = id_in(&index, "main.tf", "a");
    let result = provenance(&index, a, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(r, StopReason::Unresolved(n) if n == "var.nowhere")),
        "got {:?}",
        result.stops
    );
}

#[test]
fn an_evaluation_context_value_is_its_own_origin() {
    let (_tmp, index) = workspace(&[("infra/main.tf", "locals {\n  who = each.value\n}\n")]);
    let who = id_in(&index, "main.tf", "who");
    let result = provenance(&index, who, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(r, StopReason::Origin(o) if o.contains("each.value"))),
        "got {:?}",
        result.stops
    );
}

#[test]
fn terraform_consumers_walk_forward_through_the_same_chain() {
    let (_tmp, index) = terraform();
    let env = id_in(&index, "variables.tf", "env");
    let result = consumers(&index, env, 10).unwrap();

    assert!(
        has_hop(&result, "prefix = \"${var.env}"),
        "{:?}",
        hop_texts(&result)
    );
    assert!(
        has_hop(&result, "name   = \"${local.prefix}-app\""),
        "the value must be followed onward through local.prefix: {:?}",
        hop_texts(&result)
    );
    assert!(
        has_hop(&result, "bucket = local.name"),
        "{:?}",
        hop_texts(&result)
    );
    assert!(result.hops.iter().any(|h| h.kind == EdgeKind::Use));
}

#[test]
fn an_output_reports_its_caller_or_says_there_is_none() {
    let (_tmp, index) = terraform();
    // `output "vpc_id"` lives in the child module, which main.tf calls.
    let vpc_id = id_in(&index, "modules/network/main.tf", "vpc_id");
    let result = consumers(&index, vpc_id, 5).unwrap();
    assert!(
        has_hop(&result, "value = module.network.vpc_id"),
        "{:?}",
        hop_texts(&result)
    );

    // An output nobody calls is an external boundary, and says so.
    let (_tmp2, lonely) = workspace(&[("mod/main.tf", "output \"lonely\" {\n  value = 1\n}\n")]);
    let out = id_in(&lonely, "mod/main.tf", "lonely");
    let result = consumers(&lonely, out, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(r, StopReason::ExternalInput { .. })),
        "got {:?}",
        result.stops
    );
}

// ==================================================================== Helm

const PARENT_VALUES: &str = r#"image:
  tag: "1.0"
  repository: parent
mysql:
  image:
    tag: "8.0"
"#;

const SUBCHART_VALUES: &str = r#"image:
  tag: "5.7"
  repository: mysql
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
"#;

fn chart() -> (tempfile::TempDir, Index) {
    workspace(&[
        ("app/Chart.yaml", "name: app\nversion: 0.1.0\n"),
        ("app/values.yaml", PARENT_VALUES),
        (
            "app/values-prod.yaml",
            "mysql:\n  image:\n    tag: \"8.4\"\n",
        ),
        (
            "app/charts/mysql/Chart.yaml",
            "name: mysql\nversion: 8.0.0\n",
        ),
        ("app/charts/mysql/values.yaml", SUBCHART_VALUES),
        ("app/templates/deployment.yaml", DEPLOYMENT),
    ])
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

#[test]
fn subchart_parent_and_user_values_all_compete_with_the_winner_marked() {
    // Helm's documented order: subchart values.yaml < parent chart < -f file < --set.
    let (_tmp, index) = chart();
    let tag = key_with_path(&index, "charts/mysql/values.yaml", "image.tag");
    let result = provenance(&index, tag, 5).unwrap();

    let competition = result
        .competitions
        .iter()
        .find(|c| c.subject.contains("image.tag"))
        .unwrap_or_else(|| panic!("no competition: {:?}", result.competitions));
    assert!(competition.model.contains("--set"), "{}", competition.model);

    let sources: Vec<(&str, &str)> = competition
        .sources
        .iter()
        .map(|s| (s.precedence.label.as_str(), s.hop.text.as_str()))
        .collect();
    assert_eq!(
        sources,
        vec![
            ("user-supplied -f values-prod.yaml", "tag: \"8.4\""),
            ("parent chart values (app)", "tag: \"8.0\""),
            ("chart defaults (mysql)", "tag: \"5.7\""),
        ],
        "strongest first, nothing dropped"
    );
    assert!(competition.winner().unwrap().hop.text.contains("8.4"));
    assert_eq!(competition.losers().len(), 2);
    assert!(
        !competition.decided,
        "--set can still override anything visible"
    );
    // Every losing source keeps its own file and line.
    for source in &competition.sources {
        assert!(source.hop.line >= 1);
        assert!(source.hop.file.exists());
    }
}

#[test]
fn a_parent_chart_key_addresses_the_subchart_it_names() {
    // `mysql.image.tag` in the parent is the same value as `image.tag` in the
    // subchart, so both queries must produce the same competition.
    let (_tmp, index) = chart();
    let from_parent = key_with_path(&index, "app/values.yaml", "mysql.image.tag");
    let result = provenance(&index, from_parent, 5).unwrap();
    let competition = result
        .competitions
        .iter()
        .find(|c| c.subject.contains("image.tag"))
        .expect("a competition");
    assert_eq!(competition.sources.len(), 3, "{:?}", competition.sources);
    assert!(competition.winner().unwrap().hop.text.contains("8.4"));
}

#[test]
fn a_key_with_one_source_reports_the_external_override_channel() {
    let (_tmp, index) = chart();
    let repository = key_with_path(&index, "app/values.yaml", "image.repository");
    let result = provenance(&index, repository, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { required: false, sources, .. } if sources.contains("--set")
        )),
        "got {:?}",
        result.stops
    );
    assert!(
        result.stopped_because(|r| matches!(r, StopReason::Origin(o) if o.contains("parent"))),
        "the literal value is the origin: {:?}",
        result.stops
    );
}

#[test]
fn two_user_supplied_files_leave_the_winner_undecided() {
    // `-f a.yaml -f b.yaml` is decided by command-line order, which no file shows.
    let (_tmp, index) = workspace(&[
        ("app/Chart.yaml", "name: app\nversion: 0.1.0\n"),
        ("app/values.yaml", "replicas: 1\n"),
        ("app/values-a.yaml", "replicas: 2\n"),
        ("app/values-b.yaml", "replicas: 3\n"),
    ]);
    let replicas = key_with_path(&index, "app/values.yaml", "replicas");
    let result = provenance(&index, replicas, 5).unwrap();

    let competition = &result.competitions[0];
    assert_eq!(competition.sources.len(), 3);
    assert!(
        competition.winner().is_none(),
        "a tie must not be resolved by guessing: {:?}",
        competition.sources
    );
    assert!(result.stopped_because(|r| matches!(r, StopReason::PrecedenceUndetermined(_))));
}

#[test]
fn a_value_read_inside_a_template_action_is_render_dependent() {
    // The action's bytes are masked out before the YAML parse, so the read is
    // structurally invisible: it is reported as such, never silently dropped.
    let (_tmp, index) = chart();
    let image = key_with_path(
        &index,
        "templates/deployment.yaml",
        "spec.template.spec.containers.image",
    );
    let result = provenance(&index, image, 6).unwrap();

    assert!(
        result.stopped_because(
            |r| matches!(r, StopReason::RenderDependent(a) if a.contains(".Values.image.tag"))
        ),
        "got {:?}",
        result.stops
    );
    assert!(
        result
            .hops
            .iter()
            .any(|h| h.kind == EdgeKind::TemplateAction),
        "{:?}",
        hop_texts(&result)
    );
    // The textual link still lands on the real values key, at a lower confidence.
    assert!(has_hop(&result, "tag: \"1.0\""), "{:?}", hop_texts(&result));
    assert!(result
        .hops
        .iter()
        .filter(|h| h.kind == EdgeKind::TemplateAction)
        .all(|h| !h.confidence.is_safe_to_rewrite()));
}

#[test]
fn a_helm_builtin_object_is_an_external_input() {
    let (_tmp, index) = chart();
    let name = key_with_path(&index, "templates/deployment.yaml", "metadata.name");
    let result = provenance(&index, name, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { name, .. } if name == ".Release"
        )),
        "got {:?}",
        result.stops
    );
}

#[test]
fn a_templated_value_with_no_values_entry_is_reported_as_unset() {
    // `.Values.replicaCount` appears in the template but in no values file: the
    // chart only renders if the user supplies it.
    let (_tmp, index) = chart();
    let replicas = key_with_path(&index, "templates/deployment.yaml", "spec.replicas");
    let result = provenance(&index, replicas, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { name, required: true, .. } if name == ".Values.replicaCount"
        )),
        "got {:?}",
        result.stops
    );
}

#[test]
fn helm_consumers_find_the_templates_that_read_a_key() {
    let (_tmp, index) = chart();
    let tag = key_with_path(&index, "app/values.yaml", "image.tag");
    let result = consumers(&index, tag, 5).unwrap();
    assert!(
        result
            .hops
            .iter()
            .any(|h| h.kind == EdgeKind::TemplateAction && h.text.contains(".Values.image.tag")),
        "{:?}",
        hop_texts(&result)
    );
    assert!(result.stopped_because(|r| matches!(r, StopReason::RenderDependent(_))));
}

#[test]
fn a_key_no_template_reads_says_so_instead_of_reporting_nothing() {
    let (_tmp, index) = chart();
    let repository = key_with_path(&index, "charts/mysql/values.yaml", "image.repository");
    let result = consumers(&index, repository, 5).unwrap();
    assert!(
        result.stopped_because(
            |r| matches!(r, StopReason::Origin(o) if o.contains("no template action"))
        ),
        "got {:?}",
        result.stops
    );
}

// ==================================================================== YAML

const ANCHORED: &str = r#"defaults: &base
  retries: 3
  timeout: 30

service:
  <<: *base
  type: ClusterIP
"#;

#[test]
fn an_alias_takes_its_value_from_its_anchor() {
    let (_tmp, index) = workspace(&[("conf/app.yaml", ANCHORED)]);
    let service = id_in(&index, "app.yaml", "service");
    let result = provenance(&index, service, 5).unwrap();

    assert!(
        result.hops.iter().any(|h| h.kind == EdgeKind::Expansion),
        "the alias hop must be recorded: {:?}",
        hop_texts(&result)
    );
    assert!(
        result
            .stopped_because(|r| matches!(r, StopReason::Origin(o) if o.contains("anchor &base"))),
        "got {:?}",
        result.stops
    );
}

#[test]
fn an_anchors_consumers_are_its_aliases() {
    let (_tmp, index) = workspace(&[("conf/app.yaml", ANCHORED)]);
    let anchor = index
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Anchor)
        .unwrap();
    let result = consumers(&index, anchor.id, 5).unwrap();
    assert!(
        result.hops.iter().any(|h| h.text == "*base"),
        "{:?}",
        hop_texts(&result)
    );
}

#[test]
fn a_plain_yaml_key_reports_its_literal_origin() {
    let (_tmp, index) = workspace(&[("conf/app.yaml", "image:\n  tag: \"1.2\"\n")]);
    let tag = id_in(&index, "app.yaml", "tag");
    let result = provenance(&index, tag, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(r, StopReason::Origin(o) if o.contains("1.2"))),
        "got {:?}",
        result.stops
    );
}

// ===================================================================== CSS

const STYLES: &str = r#".btn { color: red; padding: 1px; }
#main .btn { color: blue; }
button.btn { color: green; }
"#;

#[test]
fn the_cascade_orders_competing_declarations_by_specificity() {
    // #id beats .class beats element, and every loser stays visible — this is the
    // DevTools struck-through view, not a single answer.
    let (_tmp, index) = workspace(&[("ui/app.css", STYLES)]);
    let btn = id_in(&index, "app.css", "btn");
    let result = provenance(&index, btn, 5).unwrap();

    // The chain starts at the symbol that was asked about.
    assert_eq!(result.hops[0].symbol, Some(btn));
    assert_eq!(result.hops[0].kind, EdgeKind::Declaration);

    let color = result
        .competitions
        .iter()
        .find(|c| c.subject.starts_with("color"))
        .unwrap_or_else(|| panic!("no color competition: {:?}", result.competitions));

    let ordered: Vec<(String, String)> = color
        .sources
        .iter()
        .map(|s| {
            (
                s.hop.text.clone(),
                s.precedence.specificity.unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        ordered,
        vec![
            ("color: blue;".to_string(), "(1,1,0)".to_string()),
            ("color: green;".to_string(), "(0,1,1)".to_string()),
            ("color: red;".to_string(), "(0,1,0)".to_string()),
        ],
        "strongest first"
    );
    let winner = color.winner().expect("a winner");
    assert!(winner.hop.text.contains("blue"));
    assert_eq!(color.losers().len(), 2);
    assert!(
        color.losers().iter().all(|l| l.reason.contains("loses to")),
        "{:?}",
        color.losers()
    );
    assert!(color.decided, "the cascade decides this one outright");

    // A property declared once is still reported, with no rivals.
    let padding = result
        .competitions
        .iter()
        .find(|c| c.subject.starts_with("padding"))
        .expect("padding competition");
    assert_eq!(padding.sources.len(), 1);
    assert!(padding.winner().is_some());
}

#[test]
fn important_beats_higher_specificity() {
    let (_tmp, index) = workspace(&[(
        "ui/app.css",
        "#main .btn { color: blue; }\n.btn { color: red !important; }\n",
    )]);
    let btn = id_in(&index, "app.css", "btn");
    let result = provenance(&index, btn, 5).unwrap();
    let color = &result.competitions[0];
    let winner = color.winner().expect("a winner");
    assert!(winner.hop.text.contains("red"), "{}", winner.hop.text);
    assert!(winner.precedence.important);
}

#[test]
fn source_order_decides_a_specificity_tie_within_one_file() {
    let (_tmp, index) = workspace(&[(
        "ui/app.css",
        ":root { --brand: red; }\n.dark { --brand: black; }\n",
    )]);
    let brand = index
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Property)
        .unwrap();
    let result = provenance(&index, brand.id, 5).unwrap();
    let competition = &result.competitions[0];
    assert_eq!(competition.sources.len(), 2);
    assert!(
        competition
            .winner()
            .expect("later declaration wins")
            .hop
            .text
            .contains("black"),
        "{:?}",
        competition.sources
    );
}

#[test]
fn a_tie_across_two_stylesheets_is_left_undecided() {
    // Which sheet loads last is a property of the document, not of the CSS.
    let (_tmp, index) = workspace(&[
        ("ui/a.css", ".btn { color: red; }\n"),
        ("ui/b.css", ".btn { color: blue; }\n"),
    ]);
    let btn = id_in(&index, "a.css", "btn");
    let result = provenance(&index, btn, 5).unwrap();
    let color = &result.competitions[0];
    assert_eq!(color.sources.len(), 2);
    assert!(color.winner().is_none(), "{:?}", color.sources);
    assert!(!color.decided);
    assert!(
        result.stopped_because(
            |r| matches!(r, StopReason::PrecedenceUndetermined(m) if m.contains("load order"))
        ),
        "got {:?}",
        result.stops
    );
}

#[test]
fn an_unlayered_declaration_beats_a_layered_one() {
    // Per the cascade, unlayered author styles win over layered ones for normal
    // declarations, regardless of specificity.
    let (_tmp, index) = workspace(&[(
        "ui/app.css",
        "@layer base {\n  #main .btn { color: blue; }\n}\n.btn { color: red; }\n",
    )]);
    let btn = symbol_in(&index, "app.css", "btn");
    let result = provenance(&index, btn.id, 5).unwrap();
    let color = &result.competitions[0];
    let winner = color.winner().expect("unlayered wins");
    assert!(winner.hop.text.contains("red"), "{}", winner.hop.text);
    assert!(winner.precedence.layer.is_none());
    assert_eq!(
        color
            .losers()
            .iter()
            .filter_map(|l| l.precedence.layer.clone())
            .collect::<Vec<_>>(),
        vec!["base".to_string()]
    );
}

#[test]
fn two_different_layers_leave_the_winner_undecided() {
    let (_tmp, index) = workspace(&[(
        "ui/app.css",
        "@layer base {\n  .btn { color: blue; }\n}\n@layer theme {\n  .btn { color: red; }\n}\n",
    )]);
    let btn = symbol_in(&index, "app.css", "btn");
    let result = provenance(&index, btn.id, 5).unwrap();
    let color = &result.competitions[0];
    assert!(color.winner().is_none(), "{:?}", color.sources);
    assert!(
        result.stopped_because(
            |r| matches!(r, StopReason::PrecedenceUndetermined(m) if m.contains("@layer"))
        ),
        "got {:?}",
        result.stops
    );
}

#[test]
fn a_conditional_declaration_is_reported_as_conditional() {
    let (_tmp, index) = workspace(&[(
        "ui/app.css",
        ".btn { color: red; }\n@media (min-width: 40em) {\n  #main .btn { color: blue; }\n}\n",
    )]);
    let btn = symbol_in(&index, "app.css", "btn");
    let result = provenance(&index, btn.id, 5).unwrap();
    let color = &result.competitions[0];
    assert!(
        !color.decided,
        "a media query is decided by the viewport, not the stylesheet"
    );
    assert!(
        color
            .sources
            .iter()
            .any(|s| s.reason.contains("@media (min-width: 40em)")),
        "{:?}",
        color.sources
    );
}

#[test]
fn a_custom_property_chain_is_followed_through_var() {
    let (_tmp, index) = workspace(&[(
        "ui/app.css",
        ":root { --brand: red; --accent: var(--brand); }\n.btn { color: var(--accent); }\n",
    )]);
    let accent = index.symbols.iter().find(|s| s.name == "--accent").unwrap();
    let result = provenance(&index, accent.id, 5).unwrap();

    assert!(
        result.hops.iter().any(|h| h.kind == EdgeKind::VarFunction),
        "{:?}",
        hop_texts(&result)
    );
    assert!(
        result
            .competitions
            .iter()
            .any(|c| c.subject.contains("--brand")),
        "the chain must reach --brand: {:?}",
        result.competitions
    );
    assert!(
        result.stopped_because(|r| matches!(r, StopReason::Origin(o) if o.contains("red"))),
        "got {:?}",
        result.stops
    );
}

#[test]
fn a_var_chain_respects_the_depth_limit() {
    let (_tmp, index) = workspace(&[(
        "ui/app.css",
        ":root { --a: var(--b); --b: var(--c); --c: 1px; }\n",
    )]);
    let a = index.symbols.iter().find(|s| s.name == "--a").unwrap();
    let result = provenance(&index, a.id, 1).unwrap();
    assert!(
        result
            .stops
            .iter()
            .any(|(_, r)| *r == StopReason::DepthLimit),
        "got {:?}",
        result.stops
    );
}

#[test]
fn an_undeclared_custom_property_is_reported_unresolved() {
    let (_tmp, index) = workspace(&[("ui/app.css", ":root { --a: var(--never-declared); }\n")]);
    let a = index.symbols.iter().find(|s| s.name == "--a").unwrap();
    let result = provenance(&index, a.id, 5).unwrap();
    assert!(
        result.stopped_because(
            |r| matches!(r, StopReason::Unresolved(n) if n.contains("--never-declared"))
        ),
        "got {:?}",
        result.stops
    );
}

#[test]
fn css_consumers_find_var_uses_and_carry_on() {
    let (_tmp, index) = workspace(&[(
        "ui/app.css",
        ":root { --brand: red; --accent: var(--brand); }\n.btn { color: var(--accent); }\n",
    )]);
    let brand = index.symbols.iter().find(|s| s.name == "--brand").unwrap();
    let result = consumers(&index, brand.id, 5).unwrap();
    assert!(
        has_hop(&result, "--accent: var(--brand);"),
        "{:?}",
        hop_texts(&result)
    );
    assert!(
        has_hop(&result, "color: var(--accent);"),
        "the chain continues through --accent: {:?}",
        hop_texts(&result)
    );
}

#[test]
fn a_class_used_from_html_is_reported_as_a_consumer() {
    let (_tmp, index) = workspace(&[
        ("ui/app.css", ".btn { color: red; }\n"),
        ("ui/index.html", "<button class=\"btn\">go</button>\n"),
    ]);
    let btn = id_in(&index, "app.css", "btn");
    let result = consumers(&index, btn, 5).unwrap();
    assert!(
        result.hops.iter().any(|h| h.file.ends_with("index.html")),
        "{:?}",
        hop_texts(&result)
    );
}

// ============================================================ refusals

#[test]
fn imperative_languages_are_refused_and_pointed_at_flow() {
    let (tmp, index) = workspace(&[("src/main.rs", "fn f() {\n    let a = 1;\n}\n")]);
    let a = index.symbols.iter().find(|s| s.name == "a").unwrap();

    let error = provenance(&index, a.id, 5).unwrap_err().to_string();
    assert!(error.contains("analysis::flow"), "{error}");
    assert!(error.contains("imperative"), "{error}");
    assert!(consumers(&index, a.id, 5).is_err());

    // The two analyses partition the languages exactly.
    let rust = tmp.path().join("src/main.rs");
    assert!(flow::applies_to(&index, &rust));
    assert!(!prov::applies_to(&index, &rust));
}

#[test]
fn provenance_and_flow_split_the_languages_between_them() {
    let (tmp, index) = workspace(&[
        ("a/main.tf", "locals {\n  x = 1\n}\n"),
        ("a/app.css", ".btn { color: red; }\n"),
        ("a/conf.yaml", "a: 1\n"),
        ("a/main.rs", "fn f() {}\n"),
    ]);
    for (name, is_config) in [
        ("a/main.tf", true),
        ("a/app.css", true),
        ("a/conf.yaml", true),
        ("a/main.rs", false),
    ] {
        let path = tmp.path().join(name);
        assert_eq!(prov::applies_to(&index, &path), is_config, "{name}");
        assert_eq!(flow::applies_to(&index, &path), !is_config, "{name}");
    }
}

#[test]
fn a_config_language_with_no_substitution_model_says_so() {
    let (_tmp, index) = workspace(&[("docs/readme.md", "# Title\n\ntext\n")]);
    let heading = index
        .symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Heading)
        .unwrap();
    let result = provenance(&index, heading.id, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(r, StopReason::UnsupportedLanguage(_))),
        "got {:?}",
        result.stops
    );
    assert!(result.is_empty());
}

#[test]
fn an_unknown_symbol_id_is_an_error_not_an_empty_answer() {
    let (_tmp, index) = workspace(&[("a/main.tf", "locals {\n  x = 1\n}\n")]);
    assert!(provenance(&index, SymbolId(9999), 5).is_err());
}

// ============================================================ presentation

#[test]
fn format_tree_shows_hops_competitions_and_stops() {
    let (_tmp, index) = terraform();
    let name = id_in(&index, "main.tf", "name");
    let rendered = provenance(&index, name, 10).unwrap().format_tree();
    assert!(rendered.contains("local.name"), "{rendered}");
    assert!(rendered.contains("substitution"), "{rendered}");
    assert!(rendered.contains("WINS"), "{rendered}");
    assert!(rendered.contains("loses"), "{rendered}");
    assert!(rendered.contains("Stopped at:"), "{rendered}");
}

#[test]
fn specificity_is_exposed_for_callers_that_need_it() {
    assert!(specificity("#a") > specificity(".a"));
    assert!(specificity(".a") > specificity("a"));
}

#[test]
fn a_child_module_input_comes_from_its_caller_not_from_tfvars() {
    // `-var` and `*.tfvars` reach the root module only: a child module's inputs are
    // the arguments its caller passes, so the chain must cross the module boundary.
    let (_tmp, index) = workspace(&[
        (
            "infra/main.tf",
            "variable \"region\" {\n  default = \"eu-west-1\"\n}\n\nmodule \"network\" {\n  source = \"./modules/network\"\n  cidr   = var.region\n}\n",
        ),
        (
            "infra/modules/network/main.tf",
            "variable \"cidr\" {\n  default = \"10.0.0.0/8\"\n}\n",
        ),
        ("infra/terraform.tfvars", "cidr = \"ignored\"\n"),
    ]);
    let cidr = id_in(&index, "modules/network/main.tf", "cidr");
    let result = provenance(&index, cidr, 5).unwrap();

    let competition = &result.competitions[0];
    assert!(
        competition.model.contains("root module only"),
        "{}",
        competition.model
    );
    let winner = competition.winner().expect("the caller's argument wins");
    assert!(
        winner.hop.text.contains("cidr = var.region"),
        "{}",
        winner.hop.text
    );
    assert!(winner.hop.file.ends_with("infra/main.tf"));
    assert!(
        competition
            .losers()
            .iter()
            .any(|l| l.hop.text.contains("10.0.0.0/8")),
        "the default must stay visible: {:?}",
        competition.losers()
    );
    assert!(
        competition
            .sources
            .iter()
            .all(|s| !s.hop.text.contains("ignored")),
        "a root-module tfvars entry must not be offered as a source: {:?}",
        competition.sources
    );

    // And the caller's expression keeps going.
    assert!(
        has_hop(&result, "var.region = \"eu-west-1\""),
        "{:?}",
        hop_texts(&result)
    );
    assert!(competition.decided, "one caller, one value");
}

#[test]
fn a_module_called_twice_reports_both_instances_and_picks_neither() {
    let (_tmp, index) = workspace(&[
        (
            "infra/a.tf",
            "module \"one\" {\n  source = \"./mod\"\n  size   = 1\n}\n",
        ),
        (
            "infra/b.tf",
            "module \"two\" {\n  source = \"./mod\"\n  size   = 2\n}\n",
        ),
        ("infra/mod/main.tf", "variable \"size\" {\n}\n"),
    ]);
    let size = id_in(&index, "mod/main.tf", "size");
    let result = provenance(&index, size, 5).unwrap();

    let competition = &result.competitions[0];
    assert_eq!(competition.sources.len(), 2, "{:?}", competition.sources);
    assert!(
        competition.winner().is_none(),
        "two instantiations are not an override: {:?}",
        competition.sources
    );
    assert!(result.stopped_because(
        |r| matches!(r, StopReason::PrecedenceUndetermined(m) if m.contains("separate instance"))
    ));
}

#[test]
fn a_child_module_input_the_caller_never_passes_is_reported() {
    let (_tmp, index) = workspace(&[
        ("infra/main.tf", "module \"m\" {\n  source = \"./mod\"\n}\n"),
        (
            "infra/mod/main.tf",
            "variable \"needed\" {\n  type = string\n}\n",
        ),
    ]);
    let needed = id_in(&index, "mod/main.tf", "needed");
    let result = provenance(&index, needed, 5).unwrap();
    assert!(
        result.stopped_because(|r| matches!(
            r,
            StopReason::ExternalInput { required: true, sources, .. } if sources.contains("passes no")
        )),
        "got {:?}",
        result.stops
    );
    assert!(result.competitions.is_empty());
}
