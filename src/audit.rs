use crate::application_ir::{Adapter, FeatureKind};
use crate::capabilities::{self, Capability};
use crate::lang::{Language, LanguageClass};
use anyhow::{ensure, Result};
use clap::{Args, ValueEnum};
use serde_json::{json, Value};

pub const SCHEMA: &str = "fr-completion-audit-1";

#[derive(Args)]
pub struct Options {
    #[arg(value_enum, default_value_t = Section::Summary)]
    pub section: Section,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum Section {
    Summary,
    Capabilities,
    Workflows,
    Recipes,
    Semantic,
    Frameworks,
    Proofs,
    Boundaries,
}

impl Section {
    pub fn name(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Capabilities => "capabilities",
            Self::Workflows => "workflows",
            Self::Recipes => "recipes",
            Self::Semantic => "semantic",
            Self::Frameworks => "frameworks",
            Self::Proofs => "proofs",
            Self::Boundaries => "boundaries",
        }
    }

    pub const DETAILS: [Self; 7] = [
        Self::Capabilities,
        Self::Workflows,
        Self::Recipes,
        Self::Semantic,
        Self::Frameworks,
        Self::Proofs,
        Self::Boundaries,
    ];
}

#[derive(Clone, Copy)]
struct Workflow {
    id: &'static str,
    purposes: &'static [&'static str],
    predicate: &'static str,
    operation_kinds: &'static [&'static str],
    output_schemas: &'static [&'static str],
    source: &'static str,
    acceptance_target: &'static str,
}

const WORKFLOWS: &[Workflow] = &[
    Workflow {
        id: "evidence",
        purposes: &["understand", "trace"],
        predicate: "project-selection",
        operation_kinds: &["project-query"],
        output_schemas: &["fr-agent-context-1", "fr-project-1"],
        source: "Source-free. Exact source is a separate bounded reveal.",
        acceptance_target: "agent_guide_cli::guide_resolves_names_and_compiles_source_free_evidence_deterministically",
    },
    Workflow {
        id: "direct-capability",
        purposes: &["understand", "trace", "change"],
        predicate: "capabilities::support + Capability::agent_form",
        operation_kinds: &["capability"],
        output_schemas: &["command-specific preview or query"],
        source: "The selected live capability form determines source use.",
        acceptance_target: "cli::tests::every_live_agent_capability_form_is_accepted_by_the_same_cli",
    },
    Workflow {
        id: "recipe",
        purposes: &["change"],
        predicate: "recipe::vocabulary + capabilities::support",
        operation_kinds: &["recipe"],
        output_schemas: &["recipe schema 1"],
        source: "Only verbs whose live form requires source request a reveal.",
        acceptance_target: "agent_guide_cli::recipe_guidance_exposes_only_live_targeted_forms_and_runs_the_authored_preview",
    },
    Workflow {
        id: "semantic-scalar",
        purposes: &["change"],
        predicate: "task_author_target_candidate + semantic body identity",
        operation_kinds: &["task-change", "author-batch"],
        output_schemas: &["fr-semantic-intent-1", "fr-task-change-1"],
        source: "Source-free after semantic extraction.",
        acceptance_target: "agent_guide_cli::scalar_goal_returns_exact_plan_and_preview_without_mutation",
    },
    Workflow {
        id: "semantic-change",
        purposes: &["change"],
        predicate: "task_author_target_candidate + semantic body identity",
        operation_kinds: &["task-change", "author-batch"],
        output_schemas: &["fr-semantic-change-1", "fr-task-change-1"],
        source: "Source-free authoring over disclosed semantic IR.",
        acceptance_target: "task_change_cli::reviewed_semantic_delta_runs_checks_reversal_and_patch_delivery",
    },
    Workflow {
        id: "semantic-body",
        purposes: &["change"],
        predicate: "task_author_target_candidate + semantic body identity",
        operation_kinds: &["task-change", "author-batch"],
        output_schemas: &["fr-semantic-body-1", "fr-task-change-1"],
        source: "Source-free authoring over disclosed semantic IR.",
        acceptance_target: "task_change_cli::reviewed_task_change_accepts_and_binds_a_source_free_semantic_body",
    },
    Workflow {
        id: "surface-edit",
        purposes: &["change"],
        predicate: "surface capability predicate",
        operation_kinds: &["surface-edit"],
        output_schemas: &["fr-surface-edit-1"],
        source: "Surface facts come first. The agent authors the edit payload.",
        acceptance_target: "agent_guide_cli::formalization_and_surface_goals_load_only_relevant_workbenches",
    },
    Workflow {
        id: "framework-migration",
        purposes: &["migrate"],
        predicate: "project features + application adapter compatibility",
        operation_kinds: &["framework-migration", "application-migration"],
        output_schemas: &["fr-project-1", "fr-application-migration-1"],
        source: "Normalized feature or application IR.",
        acceptance_target: "agent_guide_cli::migration_guide_uses_the_application_ir_for_portable_backend_adapters",
    },
    Workflow {
        id: "formalization",
        purposes: &["prove"],
        predicate: "spec::formal_plan",
        operation_kinds: &["property-task", "formal-plan"],
        output_schemas: &["fr-property-task-1", "fr-formal-plan-1"],
        source: "Property authoring is source-free. The deterministic model binds source identities.",
        acceptance_target: "lean_adoption::agent_authors_a_multi_input_property_and_its_proof",
    },
    Workflow {
        id: "proof",
        purposes: &["prove"],
        predicate: "spec proof task admission",
        operation_kinds: &["proof-task", "proof-submission"],
        output_schemas: &["fr-proof-task-1", "fr-proof-attempt-1"],
        source: "Bounded goal disclosure. The agent authors the tactics.",
        acceptance_target: "agent_intent_cli::tagged_proof_workflow_checks_scaffold_and_agent_tactics_before_history",
    },
];

fn capability_report() -> Value {
    let rows = capabilities::matrix();
    let (supported, not_applicable, refused) = capabilities::totals();
    let cells = rows
        .into_iter()
        .map(|row| {
            let capability = Capability::ALL
                .iter()
                .find(|capability| capability.label() == row.capability)
                .expect("Matrix row comes from the capability catalog.");
            let form = capability.agent_form();
            json!({
                "capability": row.capability,
                "command": row.command,
                "agent_form": form.arguments,
                "writes": form.writes,
                "source_required": form.source_required,
                "languages": row.languages,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "counts": {"capabilities": Capability::ALL.len(), "languages": Language::ALL.len(),
            "cells": supported + not_applicable + refused, "supported": supported,
            "not_applicable": not_applicable, "refused": refused},
        "cells": cells,
        "claim": "Supported means the live predicate admits the language. An exact input can still refuse.",
    })
}

fn workflow_report() -> Value {
    json!({
        "goal_schema": crate::project::agent_guide::GOAL_SCHEMA,
        "guide_schema": crate::project::agent_guide::GUIDE_SCHEMA,
        "routes": WORKFLOWS.iter().map(|route| json!({
            "id": route.id, "purposes": route.purposes, "predicate": route.predicate,
            "operation_kinds": route.operation_kinds, "output_schemas": route.output_schemas,
            "source_policy": route.source, "acceptance_target": route.acceptance_target,
        })).collect::<Vec<_>>(),
        "delivery_invariants": ["complete-preview", "reviewed-basis", "unchanged-input",
            "declared-original-checks", "declared-final-checks", "patch-verification", "exact-undo-redo"],
        "validation_state": "Acceptance targets name executable tests. This report does not claim that an unrun test passed.",
        "deterministic_evaluation": {
            "schema":"fr-completion-workflows-1",
            "auditor":"python3 tools/completion-workflows.py --audit tests/agent-eval/completion-workflows.json",
            "retained":"tests/agent-eval/completion-workflows.json",
            "families":["understanding","tracing","direct-change","recipe","semantic-edit","framework-migration","proof"],
            "claim":"The retained run executes every guided preview without exploratory calls after guidance. It records protocol bytes and makes no model-token or quota claim."
        },
    })
}

fn recipe_report() -> Value {
    let vocabulary = crate::recipe::vocabulary();
    json!({
        "schema": vocabulary.schema,
        "verbs": vocabulary.verbs,
        "requirements": vocabulary.requirements,
        "expectations": vocabulary.expectations,
        "predicates": vocabulary.predicates,
        "file_predicates": vocabulary.file_predicates,
        "rewrites": vocabulary.rewrites,
        "modifiers": vocabulary.modifiers,
        "languages": vocabulary.languages,
    })
}

fn semantic_report() -> Result<Value> {
    crate::project::semantic_ir::catalog(&crate::project::semantic_ir::SchemaOptions {
        section: None,
        kind: None,
    })
}

fn framework_report() -> Value {
    let cells = Adapter::ALL
        .into_iter()
        .flat_map(|source| {
            Adapter::ALL.into_iter().flat_map(move |target| {
                FeatureKind::ALL.into_iter().map(move |feature| {
                    let admitted = crate::framework_kernel::application_adapters_compatible(
                        source.code(), target.code(), feature.code(),
                    );
                    json!({"source":source.name(), "target":target.name(), "feature":feature.name(),
                        "status":if admitted {"supported"} else {"unsupported"},
                        "source_reader":crate::framework_kernel::application_adapter_supports(source.code(), feature.code()),
                        "target_writer":crate::framework_kernel::application_adapter_supports(target.code(), feature.code()),
                        "runtime_proved":false})
                })
            })
        })
        .collect::<Vec<_>>();
    let supported = cells
        .iter()
        .filter(|cell| cell["status"] == "supported")
        .count();
    json!({
        "application_schema": crate::application_ir::SCHEMA,
        "adapters": Adapter::ALL.map(Adapter::name),
        "features": FeatureKind::ALL.map(FeatureKind::name),
        "counts":{"cells":cells.len(),"supported":supported,"unsupported":cells.len()-supported},
        "cells":cells,
        "excluded":["request bodies","query validation","middleware","authentication",
            "service calls","dynamic rendering","implicit HTTP methods","runtime configuration"],
        "claim":"Compatibility is a checked static admission policy. Runtime fixtures remain separate evidence.",
    })
}

fn proof_report() -> Value {
    let formalization = Language::ALL
        .iter()
        .map(|language| {
            let mode = if crate::transpile::can_be_read(*language) {
                "typed-pure-when-admitted"
            } else if language.class() == LanguageClass::Config {
                "structural-snapshot"
            } else {
                "unsupported"
            };
            json!({"language":language.name(),"mode":mode})
        })
        .collect::<Vec<_>>();
    json!({
        "lean_toolchain":crate::spec::LEAN_TOOLCHAIN,
        "schemas":{
            "formal_plan":crate::spec::FORMAL_PLAN_SCHEMA,
            "formal_goals":crate::spec::FORMAL_GOALS_SCHEMA,
            "property_task":crate::spec::PROPERTY_TASK_SCHEMA,
            "agent_property":crate::spec::AGENT_PROPERTY_SCHEMA,
            "proof_task":crate::spec::PROOF_TASK_SCHEMA,
            "proof_attempt":crate::spec::PROOF_ATTEMPT_SCHEMA,
        },
        "formalization":formalization,
        "evidence_classes":[
            {"name":"syntax","claim":"Generated files parse."},
            {"name":"compiler","claim":"The pinned toolchain accepts generated files."},
            {"name":"behavioral","claim":"Only the exercised cases."},
            {"name":"model-theorem","claim":"The named theorem under its stated assumptions."},
            {"name":"implementation-correspondence","claim":"Only mapped declarations with current anchors, signatures and a checked bridge."}
        ],
        "trusted_boundaries":["parser","semantic extraction","filesystem","hash implementation",
            "compiler and runtime","Git","unmodeled effects and external behavior"],
    })
}

fn boundary_report() -> Value {
    json!({
        "analysis":[
            {"id":"dynamic-call-target", "status":"inherent-static-boundary",
             "behavior":"Unsettled receivers fan out over source-supported candidates. Names assembled only at runtime remain unknown.",
             "evidence":["receiver type", "declared hierarchy", "callable-value bindings", "parse coverage"],
             "refuses_to_claim":["absence of external callers", "runtime reflection targets", "uses hidden by syntax errors"]},
            {"id":"runtime-framework-behavior", "status":"separate-evidence-required",
             "behavior":"Application IR admits only normalized supported features.",
             "refuses_to_claim":["middleware equivalence", "authentication equivalence", "dynamic rendering equivalence"]},
            {"id":"source-equivalence", "status":"not-established-generally",
             "behavior":"The report separates model theorems from source correspondence."}
        ],
        "rule":"An unavailable static fact is uncertainty, not absence and not an actionable defect.",
    })
}

fn summary_report() -> Value {
    let (supported, not_applicable, refused) = capabilities::totals();
    let framework_cells = Adapter::ALL.len() * Adapter::ALL.len() * FeatureKind::ALL.len();
    let framework_supported = Adapter::ALL
        .into_iter()
        .flat_map(|source| {
            Adapter::ALL.into_iter().flat_map(move |target| {
                FeatureKind::ALL.into_iter().map(move |feature| {
                    crate::framework_kernel::application_adapters_compatible(
                        source.code(),
                        target.code(),
                        feature.code(),
                    )
                })
            })
        })
        .filter(|supported| *supported)
        .count();
    json!({
        "counts":{
            "parser_languages":Language::ALL.len(), "capabilities":Capability::ALL.len(),
            "capability_cells":supported+not_applicable+refused,
            "supported_capability_cells":supported,
            "refused_or_inapplicable_capability_cells":not_applicable+refused,
            "workflow_routes":WORKFLOWS.len(), "recipe_verbs":crate::recipe::vocabulary().verbs.len(),
            "application_cells":framework_cells, "supported_application_cells":framework_supported,
        },
        "claims":{
            "support":"Live predicates determine support. Exact inputs may still refuse.",
            "validation":"Named test targets and gates are obligations until they run on this revision.",
            "proof":"Model, implementation correspondence and runtime behavior remain distinct."
        },
        "reveal":Section::DETAILS.map(|section| json!({"section":section.name(),"arguments":["audit",section.name()]})),
    })
}

pub fn report(section: Section) -> Result<Value> {
    let detail = match section {
        Section::Summary => summary_report(),
        Section::Capabilities => capability_report(),
        Section::Workflows => workflow_report(),
        Section::Recipes => recipe_report(),
        Section::Semantic => semantic_report()?,
        Section::Frameworks => framework_report(),
        Section::Proofs => proof_report(),
        Section::Boundaries => boundary_report(),
    };
    let mut report = json!({
        "schema":SCHEMA,
        "section":section.name(),
        "sources":["live predicates","live schema constants","live catalogs","acceptance registry"],
        "report":detail,
    });
    report["object_root"] = json!(crate::project::object_merkle(&report)?);
    ensure!(
        serde_json::to_vec(&report)?.len() <= 256 * 1024,
        "Completion audit section exceeds its 256 KiB bound."
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_is_bounded_and_content_addressed() {
        for section in std::iter::once(Section::Summary).chain(Section::DETAILS) {
            let report = report(section).unwrap();
            let root = report["object_root"].as_str().unwrap();
            assert_eq!(root.len(), 64);
            assert!(root.bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert!(serde_json::to_vec(&report).unwrap().len() <= 256 * 1024);
        }
    }

    #[test]
    fn every_support_cell_is_decided_and_every_route_has_acceptance_evidence() {
        let evidence = [
            include_str!("cli.rs"),
            include_str!("../tests/agent_guide_cli.rs"),
            include_str!("../tests/agent_intent_cli.rs"),
            include_str!("../tests/task_change_cli.rs"),
            include_str!("../tests/lean_adoption.rs"),
        ]
        .join("\n");
        for capability in Capability::ALL {
            for language in Language::ALL {
                match capabilities::support(*capability, *language) {
                    crate::capabilities::Support::Yes => {}
                    crate::capabilities::Support::NotApplicable { because }
                    | crate::capabilities::Support::Refused { because } => {
                        assert!(!because.is_empty());
                    }
                }
            }
        }
        assert_eq!(WORKFLOWS.len(), 10);
        for route in WORKFLOWS {
            assert!(!route.purposes.is_empty());
            assert!(!route.operation_kinds.is_empty());
            assert!(route.acceptance_target.contains("::"));
            let function = route.acceptance_target.rsplit("::").next().unwrap();
            assert!(
                evidence.contains(&format!("fn {function}(")),
                "{} names missing acceptance target {}",
                route.id,
                route.acceptance_target
            );
        }
    }

    #[test]
    fn framework_counts_are_derived_from_the_same_policy() {
        let report = framework_report();
        assert_eq!(report["counts"]["cells"], 75);
        let counted = report["cells"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|cell| cell["status"] == "supported")
            .count();
        assert_eq!(report["counts"]["supported"], counted);
    }
}
