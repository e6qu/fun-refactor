use super::{agent_intent::Purpose, semantic, task, Project, RelationshipOptions};
use crate::capabilities::{self, Capability};
use crate::lang::Language;
use crate::model::SymbolKind;
use anyhow::{ensure, Context, Result};
use clap::{Args, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub const GOAL_SCHEMA: &str = "fr-agent-goal-1";
pub const GUIDE_SCHEMA: &str = "fr-agent-guide-1";

#[derive(Args)]
pub struct Options {
    #[arg(
        long,
        help = "Structured fr-agent-goal-1 file or - for stdin; at most 64 KiB."
    )]
    from: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Goal {
    schema: String,
    purpose: Purpose,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    selector: Option<Selector>,
    #[serde(default)]
    operation: Operation,
    #[serde(default)]
    constraints: Constraints,
    #[serde(default)]
    checks: Vec<String>,
    #[serde(default)]
    proof: ProofExpectation,
    #[serde(default)]
    context: Limits,
    #[serde(default)]
    delivery: Option<super::task_change::Delivery>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Selector {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default = "workspace")]
    scope: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    language: Option<Language>,
    #[serde(default)]
    locals: bool,
}

fn workspace() -> String {
    ".".into()
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    #[default]
    Automatic,
    Capability {
        capability: Capability,
        #[serde(default)]
        parameters: BTreeMap<String, String>,
    },
    Recipe {
        verb: String,
    },
    SemanticScalar {
        operation: String,
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
    },
    SemanticChange,
    SemanticBody,
    SurfaceEdit {
        surface: String,
    },
    FrameworkMigration {
        to: String,
    },
    Formalize,
    Proof {
        obligation: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Constraints {
    #[serde(default)]
    allow_source: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ProofExpectation {
    #[default]
    None,
    Model,
    Implementation,
}

impl ProofExpectation {
    fn code(self) -> usize {
        match self {
            Self::None => 0,
            Self::Model => 1,
            Self::Implementation => 2,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Limits {
    #[serde(default = "reveal_limit")]
    token_limit: usize,
    #[serde(default = "packet_limit")]
    packet_limit: usize,
}

fn reveal_limit() -> usize {
    4096
}
fn packet_limit() -> usize {
    16384
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            token_limit: reveal_limit(),
            packet_limit: packet_limit(),
        }
    }
}

#[allow(clippy::too_many_arguments)] // Scalar inputs are shared by Rust, Python and the anchored Lean kernel.
pub fn agent_guide_route_admitted(
    purpose: usize,
    language_class: usize,
    target_kind: usize,
    route: usize,
    supported: bool,
    source_required: bool,
    source_allowed: bool,
    proof_expectation: usize,
) -> bool {
    let purpose_matches = match route {
        0 => purpose <= 4,
        1 => purpose <= 2,
        2..=6 => purpose == 2,
        7 => purpose == 3,
        8 | 9 => purpose == 4,
        _ => false,
    };
    purpose_matches
        && language_class <= 1
        && target_kind <= 2
        && supported
        && (!source_required || source_allowed)
        && proof_expectation <= 1
}

pub fn agent_guide_step(
    state: usize,
    action: usize,
    ready: bool,
    complete_review: bool,
    basis_matches: bool,
) -> usize {
    match (state, action, ready, complete_review, basis_matches) {
        (0, 0, true, _, _) => 1,
        (1, 1, true, _, _) => 2,
        (2, 2, true, true, _) => 3,
        (3, 3, true, true, true) => 4,
        _ => 5,
    }
}

fn read_goal(root: &Path, path: &Path) -> Result<Goal> {
    let mut bytes = Vec::new();
    if path == Path::new("-") {
        io::stdin().take(65_537).read_to_end(&mut bytes)?;
    } else {
        let path = root.join(path);
        ensure!(
            fs::symlink_metadata(&path)?.is_file(),
            "agent goal must be a regular file."
        );
        fs::File::open(path)?.take(65_537).read_to_end(&mut bytes)?;
    }
    ensure!(bytes.len() <= 65_536, "agent goal exceeds 64 KiB.");
    let goal: Goal =
        serde_json::from_slice(&bytes).context("agent goal must match fr-agent-goal-1.")?;
    ensure!(
        goal.schema == GOAL_SCHEMA,
        "agent goal schema must be {GOAL_SCHEMA}."
    );
    ensure!(
        goal.target.is_none() || goal.selector.is_none(),
        "choose a target or selector, not both."
    );
    if let Some(target) = &goal.target {
        ensure!(
            target.starts_with("frp1:") && target.len() <= 16_384,
            "agent goal target must be a bounded full project handle."
        );
    }
    if let Some(selector) = &goal.selector {
        ensure!(
            selector.name.is_some() != selector.path.is_some(),
            "selector needs exactly one name or path."
        );
        for value in [
            selector.name.as_ref(),
            selector.path.as_ref(),
            Some(&selector.scope),
            selector.kind.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            ensure!(
                !value.is_empty() && value.len() <= 512 && !value.contains('\0'),
                "selector fields must be bounded nonempty strings."
            );
        }
    }
    ensure!(
        (1024..=4096).contains(&goal.context.token_limit),
        "goal reveal token limit must be 1024 through 4096."
    );
    ensure!(
        (2048..=65536).contains(&goal.context.packet_limit),
        "goal packet limit must be 2048 through 65536."
    );
    ensure!(
        goal.checks.len() <= 32
            && goal.checks.iter().collect::<BTreeSet<_>>().len() == goal.checks.len(),
        "goal checks must be at most 32 unique names."
    );
    Ok(goal)
}

fn action(
    id: &str,
    arguments: Vec<String>,
    output_schema: &str,
    input: Option<Value>,
    authors: Vec<Value>,
) -> Value {
    let schema = match output_schema {
        "" => Value::Null,
        "1" => json!(1),
        value => json!(value),
    };
    json!({"id": id, "arguments": arguments, "output_schema": schema, "schema_field":"schema",
        "input": input, "author_fields": authors, "writes": false,
        "ready": authors.is_empty(), "review_required_before_execution": true})
}

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn scalar_option(flag: &str, value: &str) -> Vec<String> {
    if value.starts_with('-') {
        vec![format!("{flag}={value}")]
    } else {
        args(&[flag, value])
    }
}
fn author(name: &str, shape: &str) -> Value {
    json!({"name":name,"shape":shape})
}

impl Project<'_> {
    fn guide_selection(&self, goal: &Goal) -> Result<Vec<usize>> {
        if let Some(handle) = &goal.target {
            return Ok(vec![self.resolve_handle(handle)?]);
        }
        let Some(selector) = &goal.selector else {
            return Ok(vec![0]);
        };
        if let Some(path) = &selector.path {
            return Ok(vec![self.target(path)?]);
        }
        let scope = self.target(&selector.scope)?;
        Ok(self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(id, node)| {
                let symbol = node.symbol.and_then(|id| self.index.symbol(id))?;
                (self.within(id, scope)
                    && Some(&node.name) == selector.name.as_ref()
                    && (selector.locals || !self.local(id))
                    && selector.kind.as_ref().is_none_or(|kind| *kind == node.kind)
                    && selector
                        .language
                        .is_none_or(|language| language == symbol.language))
                .then_some(id)
            })
            .collect())
    }

    fn guide_target(&self, selected: usize) -> Value {
        let node = &self.nodes[selected];
        let symbol = node.symbol.and_then(|id| self.index.symbol(id));
        let language = symbol.map(|symbol| symbol.language).or_else(|| {
            self.index
                .file(&self.root.join(&node.path))
                .map(|file| file.language)
        });
        let position = symbol.map(|symbol| {
            let at = self.lines[&symbol.file]
                .line_col(symbol.name_span.start, &self.sources[&symbol.file]);
            format!("{}:{}:{}", node.path.display(), at.line, at.col)
        });
        json!({"handle":self.handle(selected),"name":super::bounded_text(&node.name,160),
            "kind":node.kind,"path":super::bounded_text(&node.path.to_string_lossy(),512),
            "language":language,"position":position})
    }

    fn guide_semantic(&self, selected: usize) -> Result<Value> {
        self.semantic(&semantic::Options {
            target: self.handle(selected),
            revision: None,
            declaration: None,
            body: true,
            pointers: false,
            locators: false,
            locators_only: false,
            locator_op: None,
            locator_from: None,
            intent_to: None,
            nodes: 4096,
            unsupported_source: false,
            minimal: true,
        })
    }

    pub(crate) fn agent_guide(&self, options: &Options) -> Result<Value> {
        let goal = read_goal(&self.root, &options.from)?;
        let selected = self.guide_selection(&goal)?;
        let mut report = json!({"schema":GUIDE_SCHEMA,"goal_schema":GOAL_SCHEMA,
            "revision":self.revision,"coverage":self.coverage(),"purpose":goal.purpose,
            "limits":{"reveal_token_upper_bound":goal.context.token_limit,"packet_bytes":goal.context.packet_limit},
            "source_policy":if goal.constraints.allow_source {"explicit-reveal-allowed"} else {"source-free"},
            "execution":{"admitted":false,"requires":["complete-preview","reviewed-basis","unchanged-input"]},
            "alternatives":[],"uncertainty":[],"omitted":{"source":true,"semantic_body":true,"full_vocabulary":true,"unrelated_routes":true}});
        if selected.len() != 1 {
            report["state"] = json!(if selected.is_empty() {
                "not-found"
            } else {
                "needs-selection"
            });
            report["refusals"] = json!([if selected.is_empty() {
                "selector matched no admitted declaration"
            } else {
                "selector is ambiguous; choose one exact handle."
            }]);
            report["candidates"] = json!(selected
                .iter()
                .take(8)
                .map(|id| self.guide_target(*id))
                .collect::<Vec<_>>());
            report["omitted"]["candidates"] = json!(selected.len().saturating_sub(8));
            report["actions"] = json!([]);
            return self.finish_guide(&goal, report);
        }
        let selected = selected[0];
        let target = self.guide_target(selected);
        let node = &self.nodes[selected];
        let symbol = node.symbol.and_then(|id| self.index.symbol(id));
        let language = symbol.map(|symbol| symbol.language).or_else(|| {
            self.index
                .file(&self.root.join(&node.path))
                .map(|file| file.language)
        });
        let target_kind = if symbol.is_some() {
            2
        } else if node.kind == "file" {
            1
        } else {
            0
        };
        let class = language.map_or(1, |language| {
            usize::from(language.class() == crate::lang::LanguageClass::Config)
        });
        report["target"] = target.clone();
        let technologies = self.technologies(&super::technologies::Options {
            selection: RelationshipOptions {
                target: self.handle(selected),
                revision: None,
                limit: 40,
                cursor: None,
            },
            evidence_limit: 1,
        })?;
        report["technologies"] = json!(technologies["items"].as_array().unwrap().iter().filter(|row| row["status"] == "detected").map(|row|
            json!({"id":row["id"],"evidence_count":row["evidence_count"],"evidence":row["evidence"],"evidence_omitted":row["evidence_omitted"]})).collect::<Vec<_>>());
        let checks = crate::checks::select(&self.root, &goal.checks)?;
        report["checks"] = checks.as_ref().map_or(json!({"names":[],"basis":null,"coverage":[]}), |checks|
            json!({"names":checks.checks,"basis":checks.configuration_basis,"coverage":checks.coverage}));

        let automatic = match goal.purpose {
            Purpose::Understand | Purpose::Trace => Operation::Automatic,
            Purpose::Change => Operation::SemanticChange,
            Purpose::Migrate => Operation::FrameworkMigration { to: String::new() },
            Purpose::Prove => {
                if language == Some(Language::Lean) {
                    Operation::Proof {
                        obligation: symbol.map_or_else(String::new, |symbol| symbol.name.clone()),
                    }
                } else {
                    Operation::Formalize
                }
            }
        };
        let operation = if matches!(goal.operation, Operation::Automatic) {
            &automatic
        } else {
            &goal.operation
        };
        let mut route = 0usize;
        let mut supported = true;
        let mut source_required = false;
        let mut refusals = Vec::<String>::new();
        let mut actions = Vec::<Value>::new();
        let handle = self.handle(selected);
        let path = node.path.to_string_lossy().to_string();
        let mut evidence = json!({"predicate":"project-selection","supported":true});
        let route_name;
        match operation {
            Operation::Automatic => {
                route_name = "evidence";
                if symbol.is_some() {
                    let sections: &[&str] = match goal.purpose {
                        Purpose::Trace => &["code_map", "call_traces", "sources_and_sinks"],
                        _ => &["code_map"],
                    };
                    let manifest = json!({"schema":"fr-agent-intent-1","target":handle,"purpose":goal.purpose,
                        "needs":sections.iter().map(|section|json!({"name":section,"section":section,"pointer":""})).collect::<Vec<_>>(),
                        "token_limit":goal.context.token_limit,"call_limit":192,"packet_limit":goal.context.packet_limit});
                    actions.push(action(
                        "evidence",
                        args(&["intent", "--from", "-"]),
                        "fr-agent-context-1",
                        Some(manifest),
                        vec![],
                    ));
                } else {
                    actions.push(action(
                        "structure",
                        args(&["project", "map", &handle, "--depth", "2", "--limit", "8"]),
                        "fr-project-1",
                        None,
                        vec![],
                    ));
                }
            }
            Operation::Capability {
                capability,
                parameters,
            } => {
                route = 1;
                route_name = "direct-capability";
                let form = capability.agent_form();
                source_required = form.source_required;
                let support = language.map(|language| capabilities::support(*capability, language));
                supported = support.as_ref().is_some_and(|support| support.is_yes());
                if form.arguments.contains(&"{position}") && symbol.is_none() {
                    supported = false;
                    refusals.push(
                        "this direct route requires one exact declaration or resolved reference."
                            .into(),
                    );
                }
                if let Some(reason) = support.as_ref().and_then(|support| support.reason()) {
                    refusals.push(reason.into());
                }
                if language.is_none() {
                    refusals.push(
                        "direct capabilities require one file or declaration language.".into(),
                    );
                }
                evidence = json!({"predicate":"capabilities::support","capability":capability,"support":support,
                    "form":form.arguments,"source_required":source_required});
                let known = BTreeMap::from([
                    ("handle", handle.clone()),
                    ("path", path.clone()),
                    ("position", target["position"].as_str().unwrap_or("").into()),
                    (
                        "language",
                        language.map_or("", |language| language.name()).into(),
                    ),
                ]);
                let required = form
                    .arguments
                    .iter()
                    .filter_map(|argument| argument.strip_prefix('{')?.strip_suffix('}'))
                    .filter(|name| !known.contains_key(name))
                    .collect::<BTreeSet<_>>();
                ensure!(
                    parameters
                        .keys()
                        .all(|name| required.contains(name.as_str())),
                    "capability parameters must name only fields in its live argument form."
                );
                ensure!(
                    parameters.values().all(|value| !value.is_empty()
                        && value.len() <= 4096
                        && !value.contains('\0')),
                    "capability parameter values must be bounded nonempty strings."
                );
                ensure!(
                    parameters.values().all(|value| !matches!(
                        value.split('=').next(),
                        Some("--write" | "--save-plan")
                    )),
                    "capability parameters cannot request write or plan persistence."
                );
                let mut authors = Vec::new();
                let arguments = form
                    .arguments
                    .iter()
                    .map(|argument| {
                        if let Some(name) = argument
                            .strip_prefix('{')
                            .and_then(|name| name.strip_suffix('}'))
                        {
                            if let Some(value) = known.get(name).filter(|value| !value.is_empty()) {
                                value.clone()
                            } else if let Some(value) = parameters.get(name) {
                                value.clone()
                            } else {
                                authors
                                    .push(author(name, "CLI scalar from the live capability form"));
                                format!("<{name}>")
                            }
                        } else {
                            (*argument).into()
                        }
                    })
                    .collect();
                if source_required {
                    actions.extend(self.guide_source_actions(selected, goal.context.token_limit));
                }
                actions.push(action(
                    if form.writes { "preview" } else { "inspect" },
                    arguments,
                    if form.arguments.first() == Some(&"project") {
                        "fr-project-1"
                    } else {
                        ""
                    },
                    None,
                    authors,
                ));
                if form.writes {
                    report["delivery"] = json!({"route":"save-plan-then-workflow","next_after_complete_preview":"save the unchanged plan, bind declared checks, then request workflow review.","reference":"skills/fr/references/workflow.md"});
                }
            }
            Operation::Recipe { verb } => {
                route = 2;
                route_name = "recipe";
                let vocabulary = crate::recipe::vocabulary();
                let selected_verb = vocabulary.verbs.iter().find(|row| row.name == verb);
                let matching = Capability::ALL
                    .iter()
                    .filter(|capability| {
                        capability.agent_form().arguments.first() == Some(&verb.as_str())
                    })
                    .collect::<Vec<_>>();
                source_required = matching
                    .iter()
                    .any(|capability| capability.agent_form().source_required);
                supported = selected_verb.is_some_and(|verb| match verb.acts_on {
                    "symbol" => symbol.is_some(),
                    "file" | "range" => language.is_some(),
                    "workspace" => selected == 0,
                    _ => false,
                }) && (language.is_none()
                    || matching.iter().any(|capability| {
                        language.is_some_and(|language| {
                            capabilities::support(**capability, language).is_yes()
                        })
                    }));
                let predicates = if selected_verb.is_some_and(|verb| verb.acts_on == "file") {
                    &vocabulary.file_predicates
                } else {
                    &vocabulary.predicates
                };
                evidence = json!({"predicate":"recipe::vocabulary+capabilities::support","verb":selected_verb,
                    "capabilities":matching.iter().map(|capability|json!({"capability":capability,"support":language.map(|language|capabilities::support(**capability, language))})).collect::<Vec<_>>(),
                    "author_contract":{"language":language,"target_handle":handle,
                        "selector_fields":predicates.iter().filter(|predicate|matches!(**predicate,"name"|"kind"|"lang"|"file")).collect::<Vec<_>>(),
                        "target_values":{"name":symbol.map(|symbol|&symbol.name),"kind":symbol.map(|symbol|symbol.kind.as_str()),"lang":language,"file":path},
                        "expectations":vocabulary.expectations.iter().filter(|form|form.starts_with("matched ") || form.starts_with("refusals ")).collect::<Vec<_>>(),
                        "required_review":"constrain the selected target and require exact matched count before delivery."}});
                if selected_verb.is_none() {
                    refusals.push("recipe verb is not in the live vocabulary.".into());
                } else if !supported {
                    refusals.push(
                        "recipe verb does not admit the selected target kind or language.".into(),
                    );
                }
                if source_required {
                    actions.extend(self.guide_source_actions(selected, goal.context.token_limit));
                }
                actions.push(action(
                    "recipe-preview",
                    args(&["recipe", "<recipe-file>"]),
                    "1",
                    None,
                    vec![author(
                        "recipe-file",
                        selected_verb.map_or("unsupported verb", |verb| verb.form),
                    )],
                ));
            }
            Operation::SemanticScalar {
                operation,
                from,
                to,
            } => {
                route = 3;
                route_name = "semantic-scalar";
                let forms = super::semantic_ir::catalog(&super::semantic_ir::SchemaOptions {
                    section: Some(super::semantic_ir::Section::Intent),
                    kind: None,
                })?;
                let scalar_contract = forms["contract"]["operations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|form| form["op"] == *operation);
                ensure!(
                    scalar_contract.is_some(),
                    "scalar operation must be in the live semantic intent catalog."
                );
                let semantic = self.guide_semantic(selected);
                supported = semantic
                    .as_ref()
                    .is_ok_and(|report| report["body_identity"]["status"] == "available")
                    && self.guide_body_candidate(selected, 7);
                evidence = json!({"predicate":"task_author_target_candidate+semantic-body-identity","operation":operation,"scalar_contract":scalar_contract,
                    "body_identity":semantic.as_ref().ok().map(|report|&report["body_identity"])});
                if let Err(error) = semantic {
                    refusals.push(error.to_string());
                }
                let complete_delivery = from.is_some() && to.is_some() && !goal.checks.is_empty();
                let argv_scalars = from
                    .iter()
                    .chain(to.iter())
                    .all(|value| !value.contains('\0'));
                if !argv_scalars && !complete_delivery {
                    supported = false;
                    refusals.push("NUL-containing scalar values require checked task delivery through JSON stdin; CLI arguments cannot contain them.".into());
                }
                if let (Some(from), Some(to)) = (from, to) {
                    let exact = self.semantic(&semantic::Options {
                        target: handle.clone(),
                        revision: None,
                        declaration: None,
                        body: true,
                        pointers: false,
                        locators: false,
                        locators_only: false,
                        locator_op: Some(operation.clone()),
                        locator_from: Some(from.clone()),
                        intent_to: Some(to.clone()),
                        nodes: 4096,
                        unsupported_source: false,
                        minimal: true,
                    });
                    match exact {
                        Ok(plan) => {
                            evidence["scalar_plan_basis"] =
                                plan["edit_plan"]["intent_basis"].clone()
                        }
                        Err(error) => {
                            supported = false;
                            refusals.push(error.to_string());
                        }
                    }
                    if !goal.checks.is_empty() {
                        let delivery=goal.delivery.as_ref().map_or(json!({"check-original":true,"compact-success":true,"exercise-reversal":true,"patch":null,"check-output-bytes":256}),|delivery|json!(delivery));
                        let input = json!({"schema":"fr-task-change-1","requests":[],
                            "targets":[{"id":"goal","handle":handle,"op":"edit-body-scalar","scalar":{"operation":operation,"from":from,"to":to}}],
                            "postconditions":{"files-changed":1,"edits":1,"changed-operations":1,"paths-changed":[path]},
                            "checks":goal.checks,"delivery":delivery});
                        match self.task_change_bytes(
                            &serde_json::to_vec(&input)?,
                            goal.context.token_limit,
                            goal.context.packet_limit,
                        ) {
                            Ok(prepared) => {
                                evidence["preview_plan_digest"] =
                                    prepared.report["task_basis"].clone()
                            }
                            Err(error) => {
                                supported = false;
                                refusals.push(error.to_string());
                            }
                        }
                        actions.push(action(
                            "preview",
                            args(&[
                                "task-change",
                                "--from",
                                "-",
                                "--diff-bytes",
                                &goal.context.token_limit.to_string(),
                                "--report-bytes",
                                &goal.context.packet_limit.to_string(),
                            ]),
                            "fr-task-change-1",
                            Some(input),
                            vec![],
                        ));
                        report["delivery"] = json!({"route":"reviewed-task-change","execution":"FrClient.execute(TaskReview)","requires":"complete returned task-change review and its unchanged basis."});
                        let mut direct = args(&[
                            "author",
                            "edit-body-scalar",
                            &handle,
                            "--operation",
                            operation,
                        ]);
                        direct.extend(scalar_option("--from", from));
                        direct.extend(scalar_option("--to", to));
                        if argv_scalars {
                            report["alternatives"] = json!([{"route":"direct-scalar-preview","arguments":direct,"reason":"preview only; checked lifecycle would require a separate task manifest."}]);
                        }
                    }
                }
                if !complete_delivery {
                    let mut arguments = args(&[
                        "project",
                        "semantic",
                        &handle,
                        "--body",
                        "--locator-op",
                        operation,
                        "--nodes",
                        "4096",
                        "--minimal",
                    ]);
                    if let Some(from) = from {
                        arguments.extend(scalar_option("--locator-from", from));
                    }
                    let mut authors = Vec::new();
                    if from.is_none() {
                        authors.push(author("from", "one exact returned scalar value"));
                    }
                    if let Some(to) = to {
                        ensure!(
                            from.is_some(),
                            "a requested scalar value needs its exact current value."
                        );
                        arguments.extend(scalar_option("--intent-to", to));
                    } else {
                        arguments.extend(args(&["--locators", "--locators-only"]));
                        authors.push(author("to", "new scalar of the returned category"));
                    }
                    actions.push(action(
                        "scalar-plan",
                        arguments,
                        "fr-semantic-model-1",
                        None,
                        vec![],
                    ));
                    actions.last_mut().unwrap()["schema_field"] = json!("semantic_schema");
                    let mut preview = args(&[
                        "author",
                        "edit-body-scalar",
                        &handle,
                        "--operation",
                        operation,
                    ]);
                    preview.extend(scalar_option("--from", from.as_deref().unwrap_or("<from>")));
                    preview.extend(scalar_option("--to", to.as_deref().unwrap_or("<to>")));
                    actions.push(action("preview", preview, "fr-author-1", None, authors));
                }
            }
            Operation::SemanticChange | Operation::SemanticBody => {
                let body = matches!(operation, Operation::SemanticBody);
                route = if body { 5 } else { 4 };
                route_name = if body {
                    "semantic-body"
                } else {
                    "semantic-change"
                };
                let semantic = self.guide_semantic(selected);
                supported = semantic
                    .as_ref()
                    .is_ok_and(|report| report["body_identity"]["status"] == "available")
                    && self.guide_body_candidate(selected, if body { 4 } else { 5 });
                evidence = json!({"predicate":"task_author_target_candidate+semantic-body-identity","body_identity":semantic.as_ref().ok().map(|report|&report["body_identity"])});
                if let Err(error) = semantic {
                    refusals.push(error.to_string());
                }
                actions.push(action(
                    "reveal",
                    args(&[
                        "project",
                        "disclose",
                        &handle,
                        "--token-limit",
                        &goal.context.token_limit.to_string(),
                    ]),
                    "fr-progressive-disclosure-1",
                    None,
                    vec![],
                ));
                actions.push(action(
                    "schema",
                    args(&[
                        "author",
                        "semantic-schema",
                        if body { "body" } else { "change" },
                    ]),
                    "fr-semantic-catalog-1",
                    None,
                    vec![],
                ));
                actions.push(action(
                    "preview",
                    args(&[
                        "author",
                        if body {
                            "replace-body-semantic"
                        } else {
                            "edit-body-semantic"
                        },
                        &handle,
                        "--from",
                        "<input-file>",
                    ]),
                    "fr-author-1",
                    None,
                    vec![author(
                        "input-file",
                        if body {
                            "fr-semantic-body-1 typed statement tree"
                        } else {
                            "fr-semantic-change-1 basis-bound typed operations."
                        },
                    )],
                ));
            }
            Operation::SurfaceEdit { surface } => {
                route = 6;
                route_name = "surface-edit";
                let query = match surface.as_str() {
                    "styles" => "styles",
                    "diagrams" => "diagrams",
                    _ => "",
                };
                supported = !query.is_empty()
                    && language.is_some_and(|language| match query {
                        "styles" => {
                            matches!(language, Language::Css | Language::Html | Language::Tsx)
                        }
                        "diagrams" => language == Language::Markdown,
                        _ => false,
                    });
                evidence = json!({"predicate":"project-surface-reader","surface":surface,"supported":supported});
                if !supported {
                    refusals.push(
                        "surface must select an admitted styles or Markdown/Mermaid diagram host."
                            .into(),
                    );
                }
                actions.push(action(
                    "surface",
                    args(&["project", query, &handle, "--limit", "8"]),
                    "fr-project-1",
                    None,
                    vec![],
                ));
                actions.push(action(
                    "preview",
                    args(&["author", "edit-surface", "<edit-id>", "--to", "<to>"]),
                    "fr-surface-author-1",
                    None,
                    vec![
                        author("edit-id", "exact returned fr-surface-edit-1 capability."),
                        author("to", "new supported surface scalar"),
                    ],
                ));
            }
            Operation::FrameworkMigration { to } => {
                route = 7;
                route_name = "framework-migration";
                let feature_report = self.features(&super::FeatureOptions {
                    selection: RelationshipOptions {
                        target: handle.clone(),
                        revision: None,
                        limit: 40,
                        cursor: None,
                    },
                    feature: None,
                })?;
                let rows = feature_report["items"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let applications = rows
                    .iter()
                    .filter(|row| row["kind"] == "application")
                    .filter_map(|row| {
                        Some((
                            row["id"].as_str()?,
                            row["application"]["framework"].as_str()?,
                        ))
                    })
                    .collect::<BTreeMap<_, _>>();
                let compatible = rows
                    .iter()
                    .filter(|row| row["kind"] == "feature")
                    .filter(|row| {
                        row["parent"]
                            .as_str()
                            .and_then(|parent| applications.get(parent))
                            .is_some_and(|framework| {
                                matches!(*framework, "fastapi" | "nextjs-app")
                                    && super::framework_kernel::framework_migration_supported(
                                        *framework == "fastapi",
                                        to == "fastapi",
                                    )
                            })
                    })
                    .filter_map(|row| row["id"].as_str().map(str::to_owned))
                    .collect::<Vec<_>>();
                let feature_count = compatible.len();
                supported = matches!(to.as_str(), "fastapi" | "nextjs") && feature_count > 0;
                evidence = json!({"predicate":"project::features+framework_migration_supported","target":to,"compatible_feature_count":feature_count,
                    "compatible_features":compatible.iter().take(8).collect::<Vec<_>>(),"features_omitted":feature_count.saturating_sub(8)});
                if !matches!(to.as_str(), "fastapi" | "nextjs") {
                    refusals.push("choose an advertised destination: fastapi or nextjs.".into());
                }
                if feature_count == 0 {
                    refusals.push("selected subtree has no admitted framework feature.".into());
                }
                actions.push(action(
                    "features",
                    args(&["project", "features", &handle, "--limit", "8"]),
                    "fr-project-1",
                    None,
                    vec![],
                ));
                actions.push(action(
                    "preview",
                    args(&[
                        "migrate",
                        "feature",
                        "<feature-id>",
                        "--to",
                        to,
                        "--out",
                        "<destination>",
                    ]),
                    "fr-project-1",
                    None,
                    vec![
                        author("feature-id", "one compatible revision-bound feature ID"),
                        author(
                            "destination",
                            if to == "nextjs" {
                                "normalized workspace-relative destination app directory."
                            } else {
                                "normalized workspace-relative Python module path."
                            },
                        ),
                    ],
                ));
            }
            Operation::Formalize => {
                route = 8;
                route_name = "formalization";
                let formal_target =
                    symbol.map(|symbol| format!("{}::{}", node.path.display(), symbol.name));
                let formal = formal_target
                    .as_ref()
                    .map(|target| crate::spec::formal_plan(&self.root, target, &[]));
                supported = language == Some(Language::Rust)
                    && formal.as_ref().is_some_and(|formal| formal.is_ok());
                evidence = json!({"predicate":"spec::formal_plan","plan_digest":formal.as_ref().and_then(|formal|formal.as_ref().ok()).map(|formal|&formal.object_digest),"generated_language":"rust"});
                if let Some(Err(error)) = &formal {
                    refusals.push(error.to_string());
                }
                if language != Some(Language::Rust) {
                    refusals.push("generated formalization currently admits only the conservative pure Rust subset; other languages need a manual model.".into());
                }
                if let Some(target) = formal_target {
                    actions.push(action(
                        "property-task",
                        args(&[
                            "spec",
                            "property-task",
                            &target,
                            "--token-limit",
                            &goal.context.token_limit.to_string(),
                        ]),
                        "fr-property-task-1",
                        None,
                        vec![],
                    ));
                    match crate::spec::init(&self.root, Path::new("specs")) {
                        Ok(package) => {
                            let needed = package.files.iter().any(|file| !file.existing);
                            evidence["package_initialized"] = json!(!needed);
                            if needed {
                                actions.push(action(
                                    "package-init-preview",
                                    args(&["spec", "init", "specs"]),
                                    "1",
                                    None,
                                    vec![],
                                ));
                                actions.last_mut().unwrap()["before_scaffold"] = json!("review and apply the complete package initialization through the existing history writer.");
                            }
                        }
                        Err(error) => {
                            supported = false;
                            refusals.push(error.to_string());
                        }
                    }
                    actions.push(action(
                        "formal-plan",
                        args(&[
                            "spec",
                            "plan",
                            &target,
                            "--property-from",
                            "<property-file>",
                        ]),
                        "fr-formal-plan-1",
                        None,
                        vec![author(
                            "property-file",
                            "agent-authored fr-formal-property-1 property tree; no tactics.",
                        )],
                    ));
                    actions.push(action(
                        "scaffold-preview",
                        args(&["spec", "scaffold", "--from", "<plan-file>"]),
                        "1",
                        None,
                        vec![author(
                            "plan-file",
                            "unchanged complete formal plan returned by fr.",
                        )],
                    ));
                }
            }
            Operation::Proof { obligation } => {
                route = 9;
                route_name = "proof";
                supported = language == Some(Language::Lean)
                    && !obligation.is_empty()
                    && obligation.len() <= 512
                    && !obligation.contains('\0');
                evidence = json!({"predicate":"spec-proof-task","language":language,"obligation":obligation});
                if !supported {
                    refusals.push(
                        "proof route needs one Lean file/declaration and an exact obligation name."
                            .into(),
                    );
                }
                let target = format!("{path}::{obligation}");
                if supported {
                    match crate::spec::proof_task(
                        &self.root,
                        &target,
                        goal.context.token_limit,
                        self.options.respect_ignore,
                    ) {
                        Ok(task) => evidence["task_digest"] = json!(task.object_digest),
                        Err(error) => {
                            supported = false;
                            refusals.push(error.to_string());
                        }
                    }
                }
                actions.push(action(
                    "proof-task",
                    args(&[
                        "spec",
                        "proof-task",
                        &target,
                        "--token-limit",
                        &goal.context.token_limit.to_string(),
                    ]),
                    "fr-proof-task-1",
                    None,
                    vec![],
                ));
                actions.push(action(
                    "proof-check",
                    args(&[
                        "spec",
                        "proof-check",
                        &target,
                        "--from",
                        "<tactics-file>",
                        "--token-limit",
                        &goal.context.token_limit.to_string(),
                    ]),
                    "fr-proof-attempt-1",
                    None,
                    vec![author(
                        "tactics-file",
                        "agent-authored Lean tactics without leading by.",
                    )],
                ));
                actions.push(action(
                    "proof-preview",
                    args(&["spec", "prove", &target, "--from", "<tactics-file>"]),
                    "1",
                    None,
                    vec![author(
                        "tactics-file",
                        "unchanged tactics accepted by proof-check.",
                    )],
                ));
            }
        }
        if source_required && !goal.constraints.allow_source {
            refusals.push("this route requires exact source; allow an explicit bounded reveal or choose a source-free route.".into());
        }
        if matches!(goal.proof, ProofExpectation::Implementation) {
            refusals.push(
                "no navigator route currently establishes general implementation correspondence."
                    .into(),
            );
        }
        let admitted = agent_guide_route_admitted(
            goal.purpose.code(),
            class,
            target_kind,
            route,
            supported,
            source_required,
            goal.constraints.allow_source,
            goal.proof.code(),
        );
        if !admitted && refusals.is_empty() {
            refusals.push("operation does not match the goal purpose or target support.".into());
        }
        report["route"] = json!({"id":route_name,"admitted":admitted,"evidence":evidence,"source_required":source_required});
        report["state"] = json!(if admitted {
            if actions
                .first()
                .is_some_and(|action| action["ready"] == true)
            {
                "ready"
            } else {
                "needs-authoring"
            }
        } else {
            "unsupported"
        });
        report["refusals"] = json!(refusals);
        report["actions"] = if admitted { json!(actions) } else { json!([]) };
        if admitted && route == 0 && symbol.is_some() {
            report["alternatives"] = json!([{"route":"progressive-evidence","arguments":["project","disclose",handle,"--view","evidence","--token-limit",goal.context.token_limit.to_string()],
                "reason":"use exact Merkle continuations when only one evidence branch is needed.","output_schema":"fr-progressive-disclosure-1"}]);
        }
        report["reference"] = json!(match route {
            0 => "disclosure",
            1 => "workflow",
            2 => "recipes",
            3 => "semantic-intent",
            4 => "semantic-change",
            5 => "semantic",
            6 => "surfaces",
            7 => "surfaces",
            8 | 9 => "lean",
            _ => unreachable!(),
        });
        report["verification_ladder"] = json!([
            {"level":"reparse","state":"not-run","claim":"syntax only"},
            {"level":"compiler","state":"requires-declared-check","claim":"toolchain acceptance"},
            {"level":"declared-checks","state":"not-run","names":goal.checks},
            {"level":"behavioral-oracle","state":"agent-or-project-authored","claim":"only exercised cases"},
            {"level":"model-theorem","state":if matches!(route,8|9) {"agent-property-and-tactics-required"} else {"separate-formalization-required"},"proved":false},
            {"level":"implementation-correspondence","state":"not-established","proved":false}
        ]);
        report["uncertainty"]=json!(["capability support admits a route; exact planners may still refuse its target or content.","syntax-derived technology evidence does not establish framework runtime behavior."]);
        self.finish_guide(&goal, report)
    }

    fn guide_source_actions(&self, selected: usize, limit: usize) -> Vec<Value> {
        let handle = self.handle(selected);
        if self
            .sources
            .contains_key(&self.root.join(&self.nodes[selected].path))
        {
            return vec![self.guide_source(&handle, limit)];
        }
        let mut reveal = self.guide_source("<source-handle>", limit);
        reveal["ready"] = json!(false);
        reveal["author_fields"] = json!([author("source-handle", "exact file or declaration handle from the bounded project map; directories have no source text.")]);
        vec![
            action(
                "structure",
                args(&["project", "map", &handle, "--depth", "2", "--limit", "8"]),
                "fr-project-1",
                None,
                vec![],
            ),
            reveal,
        ]
    }

    fn guide_source(&self, handle: &str, limit: usize) -> Value {
        action(
            "source-reveal",
            args(&[
                "project",
                "show",
                handle,
                "--source",
                "--bytes",
                &limit.to_string(),
            ]),
            "fr-project-1",
            None,
            vec![],
        )
    }

    fn guide_body_candidate(&self, selected: usize, operation: usize) -> bool {
        let Some(symbol) = self.nodes[selected]
            .symbol
            .and_then(|id| self.index.symbol(id))
        else {
            return false;
        };
        let language = Language::ALL
            .iter()
            .position(|language| *language == symbol.language)
            .unwrap();
        let target = match symbol.kind {
            SymbolKind::Function => 1,
            SymbolKind::Method => 2,
            SymbolKind::Variable => 3,
            _ => 6,
        };
        task::task_author_target_candidate(operation, language, target)
    }

    fn finish_guide(&self, goal: &Goal, mut report: Value) -> Result<Value> {
        if let Some(actions) = report["actions"].as_array_mut() {
            for action in actions {
                action["snapshot_revision"] = json!(self.revision);
                action["max_output_bytes"] = json!(goal.context.packet_limit);
                let mut prefix = Vec::new();
                if !self.options.respect_ignore {
                    prefix.push(json!("--no-ignore"));
                }
                if self.options.max_file_bytes != super::ScanOptions::default().max_file_bytes {
                    prefix.extend([
                        json!("--max-file-size"),
                        json!(self.options.max_file_bytes.to_string()),
                    ]);
                }
                prefix.append(action["arguments"].as_array_mut().unwrap());
                action["arguments"] = json!(prefix);
            }
        }
        let vocabulary = crate::recipe::vocabulary();
        let mut catalog = vec![super::semantic_ir::catalog(
            &super::semantic_ir::SchemaOptions {
                section: None,
                kind: None,
            },
        )?];
        for section in super::semantic_ir::Section::value_variants() {
            catalog.push(super::semantic_ir::catalog(
                &super::semantic_ir::SchemaOptions {
                    section: Some(*section),
                    kind: None,
                },
            )?);
        }
        let catalog = serde_json::to_value(catalog)?;
        report["catalog_digest"] = json!(super::object_merkle(&catalog)?);
        let capability_material=Capability::ALL.iter().map(|capability|json!({"capability":capability,"form":capability.agent_form().arguments,"writes":capability.agent_form().writes,"source_required":capability.agent_form().source_required,
            "support":Language::ALL.iter().map(|language|json!([language,capabilities::support(*capability,*language)])).collect::<Vec<_>>()})).collect::<Vec<_>>();
        let canonical = serde_json::to_vec(&serde_json::to_value(goal)?)?;
        report["goal_sha256"] = json!(hex::encode(Sha256::digest(&canonical)));
        report["object_root"] = json!(super::object_merkle(&report)?);
        report["basis"] = json!(format!(
            "frag1:{}",
            super::hash((&report, goal, capability_material, vocabulary, catalog))?
        ));
        report["serialized_bytes"] = json!(0);
        for _ in 0..5 {
            let bytes = serde_json::to_vec(&report)?.len();
            if report["serialized_bytes"] == json!(bytes) {
                break;
            }
            report["serialized_bytes"] = json!(bytes);
        }
        ensure!(
            serde_json::to_vec(&report)?.len() <= goal.context.packet_limit,
            "agent guide exceeds packet limit; increase packet_limit up to 65536."
        );
        Ok(report)
    }
}
