use super::{
    bounded_text, child_id, fact, value_text, Application, BoundaryCounts, FactEvidence,
    CONFIGURATION_CONSUMER_LIMIT, CONFIGURATION_FACT_LIMIT, EXECUTION_DEPENDENCY_FACT_LIMIT,
    LIFECYCLE_FACT_LIMIT, MIDDLEWARE_FACT_LIMIT,
};
use crate::analysis::stitch;
use crate::lang::Language;
use crate::parse::{Parsed, Parsers};
use crate::project::framework_kernel;
use crate::project::{fast_routes, Project};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

struct NextLifecycle {
    name: String,
    exported_as: String,
    line: usize,
    basis: &'static str,
}

fn ts_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| !child.is_extra())
        .collect()
}

fn node_text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

fn next_lifecycles(
    parsed: &Parsed,
    source: &str,
) -> (Vec<NextLifecycle>, Vec<(usize, &'static str)>) {
    let mut hooks = Vec::new();
    let mut gaps = Vec::new();
    let mut local_callables: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for node in ts_children(parsed.root()) {
        let node = if node.kind() == "export_statement" {
            node.child_by_field_name("declaration").unwrap_or(node)
        } else {
            node
        };
        if node.kind() == "function_declaration" {
            if let Some(name) = node.child_by_field_name("name") {
                local_callables
                    .entry(node_text(name, source).to_owned())
                    .or_default()
                    .push(node.start_position().row + 1);
            }
        } else if matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
            for binding in ts_children(node)
                .into_iter()
                .filter(|child| child.kind() == "variable_declarator")
            {
                let (Some(name), Some(value)) = (
                    binding.child_by_field_name("name"),
                    binding.child_by_field_name("value"),
                ) else {
                    continue;
                };
                if name.kind() == "identifier"
                    && matches!(value.kind(), "arrow_function" | "function_expression")
                {
                    local_callables
                        .entry(node_text(name, source).to_owned())
                        .or_default()
                        .push(binding.start_position().row + 1);
                }
            }
        }
    }
    for export in ts_children(parsed.root())
        .into_iter()
        .filter(|node| node.kind() == "export_statement")
    {
        let line = export.start_position().row + 1;
        if let Some(declaration) = export.child_by_field_name("declaration") {
            if declaration.kind() == "function_declaration" {
                if let Some(name) = declaration.child_by_field_name("name") {
                    let name = node_text(name, source);
                    if matches!(name, "register" | "onRequestError") {
                        hooks.push(NextLifecycle {
                            name: name.to_owned(),
                            exported_as: name.to_owned(),
                            line,
                            basis: "nextjs-instrumentation-function-export",
                        });
                    }
                }
                continue;
            }
            if matches!(
                declaration.kind(),
                "lexical_declaration" | "variable_declaration"
            ) {
                for binding in ts_children(declaration)
                    .into_iter()
                    .filter(|node| node.kind() == "variable_declarator")
                {
                    let (Some(name), Some(value)) = (
                        binding.child_by_field_name("name"),
                        binding.child_by_field_name("value"),
                    ) else {
                        continue;
                    };
                    let name_text = node_text(name, source);
                    if matches!(name_text, "register" | "onRequestError")
                        && matches!(value.kind(), "arrow_function" | "function_expression")
                    {
                        hooks.push(NextLifecycle {
                            name: name_text.to_owned(),
                            exported_as: name_text.to_owned(),
                            line: binding.start_position().row + 1,
                            basis: "nextjs-instrumentation-variable-export",
                        });
                    } else if matches!(name_text, "register" | "onRequestError") {
                        gaps.push((
                            line,
                            "The Next.js instrumentation export is not a direct function declaration.",
                        ));
                    }
                }
                continue;
            }
        }
        for clause in ts_children(export)
            .into_iter()
            .filter(|node| node.kind() == "export_clause")
        {
            for specifier in ts_children(clause) {
                let Some(name) = specifier.child_by_field_name("name") else {
                    continue;
                };
                let alias = specifier.child_by_field_name("alias").unwrap_or(name);
                let exported_as = node_text(alias, source);
                if !matches!(exported_as, "register" | "onRequestError") {
                    continue;
                }
                if export.child_by_field_name("source").is_some() {
                    gaps.push((
                        line,
                        "Re-exported Next.js instrumentation hooks need cross-file resolution.",
                    ));
                    continue;
                }
                let local_name = node_text(name, source);
                if local_callables.get(local_name).map(Vec::len) != Some(1) {
                    gaps.push((
                        line,
                        "The local Next.js instrumentation export has no unique direct callable declaration.",
                    ));
                    continue;
                }
                hooks.push(NextLifecycle {
                    name: local_name.to_owned(),
                    exported_as: exported_as.to_owned(),
                    line: specifier.start_position().row + 1,
                    basis: "nextjs-instrumentation-local-export",
                });
            }
        }
    }
    (hooks, gaps)
}

impl Project<'_> {
    pub(super) fn file_source(&self, relative: &Path, line: usize) -> Value {
        let handle = self
            .nodes
            .iter()
            .position(|node| node.kind == "file" && node.path == relative)
            .map(|node| self.handle(node));
        json!({
            "path": bounded_text(&relative.to_string_lossy(), 512),
            "line": line,
            "handle": handle,
        })
    }

    fn absolute_source(&self, path: &Path, line: usize) -> Result<Value> {
        Ok(self.file_source(path.strip_prefix(&self.root)?, line))
    }

    fn application_contains(&self, application: &Application, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        let Some(root) = value_text(&application.root) else {
            return false;
        };
        if application.framework == "fastapi" {
            return relative == Path::new(root);
        }
        root == "." || relative.starts_with(root)
    }

    pub(super) fn configuration_facts(
        &self,
        application: &Application,
        analysis: &stitch::Analysis,
        rows: &mut Vec<Value>,
        counts: &mut BoundaryCounts,
    ) -> Result<()> {
        let mut configuration_budget = CONFIGURATION_FACT_LIMIT;
        let mut consumer_budget = CONFIGURATION_CONSUMER_LIMIT;
        let mut configurations_omitted = 0usize;
        let mut consumers_omitted = 0usize;
        let mut declared = BTreeSet::new();
        for chain in &analysis.chains {
            declared.insert(chain.env_var.as_str());
            let consumers: Vec<_> = chain
                .reads
                .iter()
                .filter(|read| self.application_contains(application, &read.file))
                .collect();
            if !self.application_contains(application, &chain.declared_in) && consumers.is_empty() {
                continue;
            }
            if configuration_budget == 0 {
                counts.configurations_omitted += 1;
                configurations_omitted += 1;
                continue;
            }
            configuration_budget -= 1;
            counts.runtime_configurations += 1;
            let visibility = match framework_kernel::configuration_visibility(
                application.framework == "nextjs-app",
                chain.env_var.starts_with("NEXT_PUBLIC_"),
            ) {
                2 => "client-build-time-candidate",
                1 => "server-default-candidate",
                _ => "server-process-candidate",
            };
            let detail = json!({
                "name": bounded_text(&chain.env_var, 160),
                "visibility": visibility,
                "conditional_on": chain.conditional_on.as_deref().map(|value| bounded_text(value, 512)),
                "values_path": chain.values_path.as_ref().map(|parts| bounded_text(&parts.join("."), 512)),
                "workspace_consumer_count": chain.reads.len(),
                "application_consumer_count": consumers.len(),
            });
            let id = child_id(
                "frfrc1",
                &self.revision,
                &application.id,
                &json!([detail, chain.declared_in, chain.declared_line]),
            )?;
            rows.push(fact(
                "runtime-configuration",
                id.clone(),
                Some(&application.id),
                self.absolute_source(&chain.declared_in, chain.declared_line)?,
                FactEvidence::new(
                    json!("environment-declaration-chain"),
                    json!("candidate"),
                    json!("name-only"),
                    if visibility == "client-build-time-candidate" {
                        json!(["Build substitution, client inclusion, deployment precedence and runtime value remain unchecked."])
                    } else {
                        json!(["Deployment precedence and runtime value remain unchecked."])
                    },
                ),
                "configuration",
                detail,
            ));
            for consumer in consumers {
                if consumer_budget == 0 {
                    counts.configuration_consumers_omitted += 1;
                    consumers_omitted += 1;
                    continue;
                }
                consumer_budget -= 1;
                counts.configuration_consumers += 1;
                let detail = json!({
                    "name": bounded_text(&chain.env_var, 160),
                    "language": consumer.language.name(),
                });
                rows.push(fact(
                    "configuration-consumer",
                    child_id("frfrcc1", &self.revision, &id, &json!([detail, consumer.line]))?,
                    Some(&id),
                    self.absolute_source(&consumer.file, consumer.line)?,
                    FactEvidence::new(
                        json!("environment-accessor-text"),
                        json!("candidate"),
                        json!(consumer.confidence),
                        json!(["Name matching does not prove deployment identity, precedence or runtime use."]),
                    ),
                    "consumer",
                    detail,
                ));
            }
        }
        let mut unmatched: BTreeMap<&str, Vec<_>> = BTreeMap::new();
        for read in &analysis.reads {
            if !declared.contains(read.name.as_str())
                && self.application_contains(application, &read.read.file)
            {
                unmatched.entry(&read.name).or_default().push(&read.read);
            }
        }
        for (name, consumers) in unmatched {
            if configuration_budget == 0 {
                counts.configurations_omitted += 1;
                configurations_omitted += 1;
                continue;
            }
            configuration_budget -= 1;
            counts.runtime_configurations += 1;
            let detail = json!({
                "name": bounded_text(name, 160),
                "visibility": match framework_kernel::configuration_visibility(
                    application.framework == "nextjs-app",
                    name.starts_with("NEXT_PUBLIC_"),
                ) {
                    2 => "client-build-time-candidate",
                    1 => "server-default-candidate",
                    _ => "server-process-candidate",
                },
                "workspace_consumer_count": consumers.len(),
                "application_consumer_count": consumers.len(),
            });
            let id = child_id("frfrc1", &self.revision, &application.id, &detail)?;
            rows.push(fact(
                "runtime-configuration",
                id.clone(),
                Some(&application.id),
                self.absolute_source(&consumers[0].file, consumers[0].line)?,
                FactEvidence::new(
                    json!("environment-accessor-text"),
                    json!("no-observed-declaration"),
                    json!("name-only"),
                    json!(["No captured declaration supplies this environment read; deployment inputs and runtime value remain unchecked."]),
                ),
                "configuration",
                detail,
            ));
            for consumer in consumers {
                if consumer_budget == 0 {
                    counts.configuration_consumers_omitted += 1;
                    consumers_omitted += 1;
                    continue;
                }
                consumer_budget -= 1;
                counts.configuration_consumers += 1;
                let detail =
                    json!({"name": bounded_text(name, 160), "language": consumer.language.name()});
                rows.push(fact(
                    "configuration-consumer",
                    child_id(
                        "frfrcc1",
                        &self.revision,
                        &id,
                        &json!([detail, consumer.line]),
                    )?,
                    Some(&id),
                    self.absolute_source(&consumer.file, consumer.line)?,
                    FactEvidence::new(
                        json!("environment-accessor-text"),
                        json!("candidate"),
                        json!(consumer.confidence),
                        json!(["No captured declaration supplies this read."]),
                    ),
                    "consumer",
                    detail,
                ));
            }
        }
        for (path, reason) in &analysis.gaps {
            if !self.application_contains(application, path) {
                continue;
            }
            counts.configuration_gaps += 1;
            let detail = json!({"reason": bounded_text(reason, 512)});
            rows.push(fact(
                "framework-gap",
                child_id(
                    "frfg1",
                    &self.revision,
                    &application.id,
                    &json!([path, reason]),
                )?,
                Some(&application.id),
                self.absolute_source(path, 1)?,
                FactEvidence::new(
                    json!("configuration-analysis"),
                    json!("gap"),
                    Value::Null,
                    json!([bounded_text(reason, 512)]),
                ),
                "gap",
                detail,
            ));
        }
        if configurations_omitted > 0 || consumers_omitted > 0 {
            counts.configuration_gaps += 1;
            let reason = "The per-application runtime configuration limits omitted facts; narrow TARGET before relying on completeness.";
            let detail = json!({
                "reason": reason,
                "configuration_facts_emitted": CONFIGURATION_FACT_LIMIT - configuration_budget,
                "configuration_consumers_emitted": CONFIGURATION_CONSUMER_LIMIT - consumer_budget,
                "configurations_omitted": configurations_omitted,
                "configuration_consumers_omitted": consumers_omitted,
            });
            rows.push(fact(
                "framework-gap",
                child_id("frfg1", &self.revision, &application.id, &detail)?,
                Some(&application.id),
                application.source.clone(),
                FactEvidence::new(
                    json!("runtime-configuration-limit"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
        }
        Ok(())
    }

    pub(super) fn middleware_facts(
        &self,
        application: &Application,
        rows: &mut Vec<Value>,
        counts: &mut BoundaryCounts,
    ) -> Result<()> {
        if application.framework == "fastapi" {
            let Some(path) = value_text(&application.root).map(PathBuf::from) else {
                return Ok(());
            };
            let absolute = self.root.join(&path);
            let Some(source) = self.sources.get(&absolute) else {
                return Ok(());
            };
            let parsed = Parsers::new().parse(Language::Python, source)?;
            let mut middleware = fast_routes::middleware(&parsed, source);
            let total = middleware.len();
            let omitted = framework_kernel::framework_omitted(total, MIDDLEWARE_FACT_LIMIT);
            middleware.truncate(framework_kernel::framework_emitted(
                total,
                MIDDLEWARE_FACT_LIMIT,
            ));
            for (index, entry) in middleware.into_iter().enumerate() {
                counts.middleware += 1;
                let resolved = entry.name.is_some();
                let gaps = if resolved {
                    json!(["Runtime registration, middleware behavior and response ordering remain unchecked."])
                } else {
                    json!(["The middleware callable exceeds the direct name subset; runtime behavior remains unchecked."])
                };
                let detail = json!({
                    "framework": "fastapi",
                    "form": entry.form,
                    "name": entry.name.as_deref().map(|name| bounded_text(name, 160)),
                    "declaration_order": index + 1,
                    "request_order": framework_kernel::middleware_request_order(total, index),
                    "order_basis": "reverse-registration-order",
                });
                rows.push(fact(
                    "middleware",
                    child_id("frfm1", &self.revision, &application.id, &detail)?,
                    Some(&application.id),
                    self.file_source(&path, entry.line),
                    FactEvidence::new(
                        json!(entry.form),
                        json!(if resolved { "candidate" } else { "unresolved" }),
                        json!(if resolved { "name-only" } else { "unknown" }),
                        gaps,
                    ),
                    "middleware",
                    detail,
                ));
            }
            if omitted > 0 {
                counts.middleware_gaps += 1;
                counts.middleware_omitted += omitted;
                let reason = "The per-application middleware fact limit omitted registrations; narrow TARGET before relying on completeness.";
                let detail = json!({"reason": reason, "omitted": omitted});
                rows.push(fact(
                    "framework-gap",
                    child_id("frfg1", &self.revision, &application.id, &detail)?,
                    Some(&application.id),
                    application.source.clone(),
                    FactEvidence::new(
                        json!("middleware-fact-limit"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
            }
            return Ok(());
        }

        let Some(root) = value_text(&application.root) else {
            return Ok(());
        };
        let root = if root == "." {
            PathBuf::new()
        } else {
            PathBuf::from(root)
        };
        let conventions = [
            ("proxy.ts", "nextjs-proxy", false),
            ("proxy.js", "nextjs-proxy", false),
            ("src/proxy.ts", "nextjs-proxy", false),
            ("src/proxy.js", "nextjs-proxy", false),
            ("middleware.ts", "nextjs-middleware", true),
            ("middleware.js", "nextjs-middleware", true),
            ("src/middleware.ts", "nextjs-middleware", true),
            ("src/middleware.js", "nextjs-middleware", true),
        ];
        let found: Vec<_> = conventions
            .into_iter()
            .map(|(path, form, deprecated)| (root.join(path), form, deprecated))
            .filter(|(path, _, _)| self.sources.contains_key(&self.root.join(path)))
            .collect();
        for (path, form, deprecated) in &found {
            counts.middleware += 1;
            let detail = json!({
                "framework": "nextjs-app",
                "form": form,
                "name": if *deprecated { "middleware" } else { "proxy" },
                "phase": "before-filesystem-routes",
                "deprecated_convention": deprecated,
            });
            rows.push(fact(
                "middleware",
                child_id("frfm1", &self.revision, &application.id, &detail)?,
                Some(&application.id),
                self.file_source(path, 1),
                FactEvidence::new(
                    json!("nextjs-convention-file"),
                    json!("candidate"),
                    Value::Null,
                    json!(["Export shape, matcher, execution runtime and request behavior remain unchecked."]),
                ),
                "middleware",
                detail,
            ));
        }
        if found.len() > 1 {
            counts.middleware_gaps += 1;
            let reason = "Multiple Next.js proxy or legacy middleware convention files need precedence review.";
            let detail = json!({"reason": reason, "candidates": found.len()});
            rows.push(fact(
                "framework-gap",
                child_id("frfg1", &self.revision, &application.id, &detail)?,
                Some(&application.id),
                application.source.clone(),
                FactEvidence::new(
                    json!("nextjs-convention-file"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
        }
        Ok(())
    }

    pub(super) fn lifecycle_facts(
        &self,
        application: &Application,
        rows: &mut Vec<Value>,
        counts: &mut BoundaryCounts,
    ) -> Result<()> {
        if application.framework == "fastapi" {
            let Some(path) = value_text(&application.root).map(PathBuf::from) else {
                return Ok(());
            };
            let Some(source) = self.sources.get(&self.root.join(&path)) else {
                return Ok(());
            };
            let parsed = Parsers::new().parse(Language::Python, source)?;
            let mut lifecycles = fast_routes::lifecycles(&parsed, source);
            let omitted = lifecycles
                .entries
                .len()
                .saturating_sub(LIFECYCLE_FACT_LIMIT);
            lifecycles.entries.truncate(LIFECYCLE_FACT_LIMIT);
            let phases: BTreeMap<_, _> = lifecycles.entries.iter().fold(
                BTreeMap::<&str, usize>::new(),
                |mut phases, entry| {
                    *phases.entry(entry.phase).or_default() += 1;
                    phases
                },
            );
            for entry in lifecycles.entries {
                counts.lifecycle_hooks += 1;
                let ambiguous = phases.get(entry.phase).is_some_and(|count| *count > 1);
                let resolved = entry.name.is_some() && !ambiguous;
                let detail = json!({
                    "framework": "fastapi",
                    "form": entry.form,
                    "name": entry.name.as_deref().map(|name| bounded_text(name, 160)),
                    "phase": entry.phase,
                    "deprecated": entry.deprecated,
                });
                rows.push(fact(
                    "lifecycle-hook",
                    child_id(
                        "frfl1",
                        &self.revision,
                        &application.id,
                        &json!([detail, entry.line]),
                    )?,
                    Some(&application.id),
                    self.file_source(&path, entry.line),
                    FactEvidence::new(
                        json!(entry.form),
                        json!(if ambiguous {
                            "ambiguous"
                        } else if resolved {
                            "candidate"
                        } else {
                            "unresolved"
                        }),
                        json!(if resolved { "name-only" } else { "unknown" }),
                        if ambiguous {
                            json!(["Multiple hooks declare this lifecycle phase; execution order needs runtime validation."])
                        } else if entry.name.is_none() {
                            json!(["The lifecycle callable exceeds the direct name subset."])
                        } else {
                            json!(["Lifecycle execution and resource effects remain unchecked."])
                        },
                    ),
                    "lifecycle",
                    detail,
                ));
            }
            for (line, reason) in lifecycles.gaps {
                counts.lifecycle_gaps += 1;
                let detail = json!({"reason": reason});
                rows.push(fact(
                    "framework-gap",
                    child_id(
                        "frfg1",
                        &self.revision,
                        &application.id,
                        &json!([line, reason]),
                    )?,
                    Some(&application.id),
                    self.file_source(&path, line),
                    FactEvidence::new(
                        json!("fastapi-lifecycle-reader"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
            }
            if omitted > 0 {
                counts.lifecycle_gaps += 1;
                counts.lifecycle_omitted += omitted;
                let reason = "The per-application lifecycle fact limit omitted hooks; narrow TARGET before relying on completeness.";
                let detail = json!({"reason": reason, "omitted": omitted});
                rows.push(fact(
                    "framework-gap",
                    child_id("frfg1", &self.revision, &application.id, &detail)?,
                    Some(&application.id),
                    application.source.clone(),
                    FactEvidence::new(
                        json!("lifecycle-fact-limit"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
            }
            return Ok(());
        }

        let Some(root) = value_text(&application.root) else {
            return Ok(());
        };
        let root = if root == "." {
            PathBuf::new()
        } else {
            PathBuf::from(root)
        };
        let files: Vec<_> = [
            "instrumentation.ts",
            "instrumentation.js",
            "src/instrumentation.ts",
            "src/instrumentation.js",
        ]
        .into_iter()
        .map(|path| root.join(path))
        .filter(|path| self.sources.contains_key(&self.root.join(path)))
        .collect();
        if files.len() > 1 {
            counts.lifecycle_gaps += 1;
            let reason =
                "Multiple Next.js instrumentation convention files need precedence review.";
            let detail = json!({"reason": reason, "candidates": files.len()});
            rows.push(fact(
                "framework-gap",
                child_id("frfg1", &self.revision, &application.id, &detail)?,
                Some(&application.id),
                application.source.clone(),
                FactEvidence::new(
                    json!("nextjs-instrumentation-file"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
        }
        for path in files {
            let source = &self.sources[&self.root.join(&path)];
            let parsed = Parsers::new().parse(Language::TypeScript, source)?;
            if parsed.has_errors() {
                counts.lifecycle_gaps += 1;
                let reason = "The Next.js instrumentation file contains syntax errors.";
                let detail = json!({"reason": reason});
                rows.push(fact(
                    "framework-gap",
                    child_id(
                        "frfg1",
                        &self.revision,
                        &application.id,
                        &json!([path, reason]),
                    )?,
                    Some(&application.id),
                    self.file_source(&path, 1),
                    FactEvidence::new(
                        json!("nextjs-instrumentation-file"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
                continue;
            }
            let (hooks, gaps) = next_lifecycles(&parsed, source);
            if hooks.is_empty() && gaps.is_empty() {
                counts.lifecycle_gaps += 1;
                let reason = "The Next.js instrumentation file has no supported register or onRequestError export.";
                let detail = json!({"reason": reason});
                rows.push(fact(
                    "framework-gap",
                    child_id(
                        "frfg1",
                        &self.revision,
                        &application.id,
                        &json!([path, reason]),
                    )?,
                    Some(&application.id),
                    self.file_source(&path, 1),
                    FactEvidence::new(
                        json!("nextjs-instrumentation-export"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
            }
            let duplicate_names: BTreeSet<_> = hooks
                .iter()
                .filter(|hook| {
                    hooks
                        .iter()
                        .filter(|other| other.exported_as == hook.exported_as)
                        .count()
                        > 1
                })
                .map(|hook| hook.exported_as.clone())
                .collect();
            for hook in hooks {
                counts.lifecycle_hooks += 1;
                let ambiguous = duplicate_names.contains(hook.exported_as.as_str());
                let detail = json!({
                    "framework": "nextjs-app",
                    "form": "nextjs-instrumentation-export",
                    "name": bounded_text(&hook.name, 160),
                    "exported_as": hook.exported_as,
                    "phase": if hook.exported_as == "register" { "startup" } else { "request-error" },
                    "deprecated": false,
                });
                rows.push(fact(
                    "lifecycle-hook",
                    child_id(
                        "frfl1",
                        &self.revision,
                        &application.id,
                        &json!([detail, path, hook.line]),
                    )?,
                    Some(&application.id),
                    self.file_source(&path, hook.line),
                    FactEvidence::new(
                        json!(hook.basis),
                        json!(if ambiguous { "ambiguous" } else { "candidate" }),
                        json!("name-only"),
                        if ambiguous {
                            json!(["Multiple exports declare this instrumentation hook."])
                        } else {
                            json!([
                                "Hook execution, runtime selection and effects remain unchecked."
                            ])
                        },
                    ),
                    "lifecycle",
                    detail,
                ));
            }
            for (line, reason) in gaps {
                counts.lifecycle_gaps += 1;
                let detail = json!({"reason": reason});
                rows.push(fact(
                    "framework-gap",
                    child_id(
                        "frfg1",
                        &self.revision,
                        &application.id,
                        &json!([path, line, reason]),
                    )?,
                    Some(&application.id),
                    self.file_source(&path, line),
                    FactEvidence::new(
                        json!("nextjs-instrumentation-export"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn global_dependency_facts(
        &self,
        application: &Application,
        rows: &mut Vec<Value>,
        counts: &mut BoundaryCounts,
    ) -> Result<()> {
        if application.framework != "fastapi" {
            return Ok(());
        }
        let Some(path) = value_text(&application.root).map(PathBuf::from) else {
            return Ok(());
        };
        let Some(source) = self.sources.get(&self.root.join(&path)) else {
            return Ok(());
        };
        let parsed = Parsers::new().parse(Language::Python, source)?;
        let dependencies = fast_routes::global_dependencies(&parsed, source);
        for dependency in dependencies.entries {
            if counts.execution_dependencies >= EXECUTION_DEPENDENCY_FACT_LIMIT {
                counts.execution_dependencies_omitted += 1;
                continue;
            }
            counts.execution_dependencies += 1;
            let authentication_candidate = dependency.marker == "Security";
            counts.authentication_candidates += usize::from(authentication_candidate);
            let resolved = dependency.provider.is_some();
            let detail = json!({
                "binding": null,
                "provider": dependency.provider.as_deref().map(|provider| bounded_text(provider, 160)),
                "marker": dependency.marker,
                "scope": "application",
                "authentication_candidate": authentication_candidate,
            });
            rows.push(fact(
                "execution-dependency",
                child_id("frfed1", &self.revision, &application.id, &json!([detail, dependency.line]))?,
                Some(&application.id),
                self.file_source(&path, dependency.line),
                FactEvidence::new(
                    json!("fastapi-application-dependency"),
                    json!(if resolved { "candidate" } else { "unresolved" }),
                    json!(if resolved { "name-only" } else { "unknown" }),
                    if resolved {
                        json!(["Dependency execution, overrides, caching and provider behavior remain unchecked."])
                    } else {
                        json!(["The application dependency provider exceeds the direct callable subset."])
                    },
                ),
                "execution_dependency",
                detail,
            ));
        }
        for (line, reason) in dependencies.gaps {
            let detail = json!({"reason": reason});
            rows.push(fact(
                "framework-gap",
                child_id(
                    "frfg1",
                    &self.revision,
                    &application.id,
                    &json!([line, reason]),
                )?,
                Some(&application.id),
                self.file_source(&path, line),
                FactEvidence::new(
                    json!("fastapi-application-dependency"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
        }
        Ok(())
    }
}
