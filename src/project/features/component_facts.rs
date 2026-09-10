use super::{
    bounded_text, child_id, fact, Application, BoundaryCounts, FactEvidence, Feature,
    COMPONENT_DETAIL_FACT_LIMIT, COMPONENT_FACT_LIMIT,
};
use crate::lang::Language;
use crate::parse::Parsers;
use crate::project::{components, framework_kernel, Project};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

struct DetailFact<'a> {
    line: usize,
    kind: &'a str,
    prefix: &'a str,
    status: &'a str,
    confidence: &'a str,
    basis: &'a str,
    detail_name: &'a str,
    detail: Value,
    gaps: Value,
}

impl Project<'_> {
    fn effective_client_files(&self, feature: &Feature) -> Result<BTreeSet<std::path::PathBuf>> {
        let paths = feature.frontend_files.iter().cloned().collect::<Vec<_>>();
        let mut clients = Vec::new();
        let mut edges = Vec::new();
        for (index, path) in paths.iter().enumerate() {
            let source = &self.sources[&self.root.join(path)];
            let parsed = Parsers::new().parse(Language::Tsx, source)?;
            if !parsed.has_errors() && components::client_module(&parsed, source) {
                clients.push(index);
            }
            for import in components::imports(&parsed, source) {
                let files =
                    components::import_candidates(path, &import.source, &self.sources, &self.root);
                if files.len() == 1 {
                    if let Some(target) = paths.iter().position(|path| *path == files[0]) {
                        edges.push((index, target));
                    }
                }
            }
        }
        loop {
            let next = crate::project::workspace_membership_step(&clients, &edges);
            if next == clients {
                break;
            }
            clients = next;
        }
        Ok(clients
            .into_iter()
            .map(|index| paths[index].clone())
            .collect())
    }

    fn component_detail(
        &self,
        rows: &mut Vec<Value>,
        counts: &mut BoundaryCounts,
        parent: &str,
        path: &std::path::Path,
        item: DetailFact<'_>,
    ) -> Result<bool> {
        if counts.component_details >= COMPONENT_DETAIL_FACT_LIMIT {
            counts.component_details_omitted += 1;
            return Ok(false);
        }
        counts.component_details += 1;
        rows.push(fact(
            item.kind,
            child_id(
                item.prefix,
                &self.revision,
                parent,
                &json!([path, item.line, item.detail]),
            )?,
            Some(parent),
            self.file_source(path, item.line),
            FactEvidence::new(
                json!(item.basis),
                json!(item.status),
                json!(item.confidence),
                item.gaps,
            ),
            item.detail_name,
            item.detail,
        ));
        Ok(true)
    }

    fn render_resolution(
        &self,
        feature: &Feature,
        path: &Path,
        declarations: &[(Option<String>, usize)],
        imports: &[components::Import],
        target: &str,
    ) -> Result<(&'static str, &'static str, Value, Value)> {
        let local = declarations
            .iter()
            .filter(|(name, _)| name.as_deref() == Some(target))
            .collect::<Vec<_>>();
        if local.len() == 1 {
            return Ok((
                "resolved",
                "name-only",
                json!({
                    "target": bounded_text(target, 160),
                    "resolution": "same-file",
                    "target_source": self.file_source(path, local[0].1),
                }),
                json!(["Name resolution does not prove runtime rendering."]),
            ));
        }
        if local.len() > 1 {
            return Ok((
                "unresolved",
                "unknown",
                json!({"target": bounded_text(target, 160), "resolution": "ambiguous-same-file"}),
                json!(["Multiple same-file component declarations match the rendered name."]),
            ));
        }
        let bindings = imports
            .iter()
            .filter(|import| import.local == target)
            .collect::<Vec<_>>();
        if bindings.len() != 1 {
            return Ok((
                "unresolved",
                "unknown",
                json!({"target": bounded_text(target, 160), "resolution": "unresolved-import"}),
                json!(["The rendered name has no unique supported import binding."]),
            ));
        }
        let binding = bindings[0];
        let files = components::import_candidates(path, &binding.source, &self.sources, &self.root);
        if files.len() != 1 || !feature.frontend_files.contains(&files[0]) {
            return Ok((
                "unresolved",
                "unknown",
                json!({"target": bounded_text(target, 160), "resolution": "unresolved-import-target"}),
                json!([
                    "The import binding has no unique component file in this feature boundary."
                ]),
            ));
        }
        let target_path = &files[0];
        let source = &self.sources[&self.root.join(target_path)];
        let parsed = Parsers::new().parse(Language::Tsx, source)?;
        if parsed.has_errors() {
            return Ok((
                "unresolved",
                "unknown",
                json!({"target": bounded_text(target, 160), "resolution": "target-syntax-gap"}),
                json!(["The imported component file contains syntax errors."]),
            ));
        }
        let declarations = components::read(&parsed, source)
            .into_iter()
            .filter(|component| {
                if binding.imported == "default" {
                    component.default_export
                } else {
                    component.name.as_deref() == Some(binding.imported.as_str())
                }
            })
            .collect::<Vec<_>>();
        if declarations.len() != 1 {
            return Ok((
                "unresolved",
                "unknown",
                json!({"target": bounded_text(target, 160), "resolution": "ambiguous-imported-declaration"}),
                json!(["The imported name has no unique supported component declaration."]),
            ));
        }
        Ok((
            "resolved",
            "import-qualified",
            json!({
                "target": bounded_text(target, 160),
                "imported_as": bounded_text(&binding.imported, 160),
                "resolution": "relative-import",
                "target_source": self.file_source(target_path, declarations[0].line),
            }),
            json!([
                "Static import resolution does not prove runtime rendering or chunk placement."
            ]),
        ))
    }

    pub(super) fn component_facts(
        &self,
        application: &Application,
        feature: &Feature,
        rows: &mut Vec<Value>,
        counts: &mut BoundaryCounts,
    ) -> Result<()> {
        if application.framework != "nextjs-app" {
            return Ok(());
        }
        let client_files = self.effective_client_files(feature)?;
        for gap in &feature.frontend_gaps {
            counts.component_gaps += 1;
            let detail = json!({"reason": gap.reason});
            rows.push(fact(
                "framework-gap",
                child_id(
                    "frfg1",
                    &self.revision,
                    &feature.id,
                    &json!([gap.path, gap.line, gap.reason]),
                )?,
                Some(&feature.id),
                self.file_source(&gap.path, gap.line),
                FactEvidence::new(
                    json!("nextjs-relative-component-import"),
                    json!("gap"),
                    Value::Null,
                    json!([gap.reason]),
                ),
                "gap",
                detail,
            ));
        }
        if feature.frontend_gaps_omitted > 0 {
            counts.component_gaps += 1;
            let reason = "The component file gap limit omitted diagnostics.";
            let detail = json!({
                "reason": reason,
                "omitted": feature.frontend_gaps_omitted,
            });
            rows.push(fact(
                "framework-gap",
                child_id("frfg1", &self.revision, &feature.id, &detail)?,
                Some(&feature.id),
                feature.source.clone(),
                FactEvidence::new(
                    json!("component-file-gap-limit"),
                    json!("gap"),
                    Value::Null,
                    json!([reason]),
                ),
                "gap",
                detail,
            ));
        }
        for path in &feature.frontend_files {
            let source = &self.sources[&self.root.join(path)];
            let parsed = Parsers::new().parse(Language::Tsx, source)?;
            if parsed.has_errors() {
                counts.component_gaps += 1;
                let reason = "The Next.js component file contains syntax errors.";
                let detail = json!({"reason": reason});
                rows.push(fact(
                    "framework-gap",
                    child_id("frfg1", &self.revision, &feature.id, &json!([path, reason]))?,
                    Some(&feature.id),
                    self.file_source(path, 1),
                    FactEvidence::new(
                        json!("nextjs-react-component"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
                continue;
            }
            let components = components::read(&parsed, source);
            let imports = components::imports(&parsed, source);
            let declarations = components
                .iter()
                .map(|component| (component.name.clone(), component.line))
                .collect::<Vec<_>>();
            if components.is_empty() {
                counts.component_gaps += 1;
                let reason =
                    "The Next.js component file has no supported direct function component.";
                let detail = json!({"reason": reason});
                rows.push(fact(
                    "framework-gap",
                    child_id("frfg1", &self.revision, &feature.id, &json!([path, reason]))?,
                    Some(&feature.id),
                    self.file_source(path, 1),
                    FactEvidence::new(
                        json!("nextjs-react-component"),
                        json!("gap"),
                        Value::Null,
                        json!([reason]),
                    ),
                    "gap",
                    detail,
                ));
            }
            for component in components {
                if counts.components >= COMPONENT_FACT_LIMIT {
                    counts.components_omitted += 1;
                    continue;
                }
                counts.components += 1;
                let resolved = component.name.is_some();
                let effective_client = client_files.contains(path);
                let detail = json!({
                    "framework": "react",
                    "name": component.name.as_deref().map(|name| bounded_text(name, 160)),
                    "file_role": match path.file_name().and_then(|name| name.to_str()) {
                        Some("page.tsx" | "page.jsx") => "page",
                        Some("layout.tsx" | "layout.jsx") => "layout",
                        _ => "imported-component",
                    },
                    "declared_boundary": if component.client { "client" } else { "server-default" },
                    "rendering_boundary": if component.client { "client" }
                        else if effective_client { "client-transitive-candidate" }
                        else { "server-default" },
                    "properties": component.props.len(),
                    "state": component.states.len(),
                    "effects": component.effects.len(),
                    "other_hooks": component.hooks.len(),
                    "events": component.events.len(),
                    "styles": component.styles.len(),
                    "render_edges": component.renders.len(),
                });
                let id = child_id(
                    "frfco1",
                    &self.revision,
                    &feature.id,
                    &json!([path, component.line, detail]),
                )?;
                rows.push(fact(
                    "component",
                    id.clone(),
                    Some(&feature.id),
                    self.file_source(path, component.line),
                    FactEvidence::new(
                        json!("nextjs-react-function-component"),
                        json!(if resolved { "candidate" } else { "unresolved" }),
                        json!(if resolved { "name-only" } else { "unknown" }),
                        if resolved && effective_client && !component.client {
                            json!(["A captured import path reaches this file from a use-client entry; bundling and runtime rendering remain unchecked."])
                        } else if resolved {
                            json!(["JSX return syntax does not prove rendering, hydration or framework runtime identity."])
                        } else {
                            json!(["The component has no direct name."])
                        },
                    ),
                    "component",
                    detail,
                ));
                if (!component.props.is_empty() || component.props_type.is_some())
                    && self.component_detail(
                        rows,
                        counts,
                        &id,
                        path,
                        DetailFact {
                            line: component.line,
                            kind: "component-properties",
                            prefix: "frfcp1",
                            status: "candidate",
                            confidence: "name-only",
                            basis: "function-parameter-properties",
                            detail_name: "properties",
                            detail: json!({
                                "names": component.props.iter().take(64).map(|name| bounded_text(name, 160)).collect::<Vec<_>>(),
                                "names_omitted": component.props.len().saturating_sub(64),
                                "declared_type": component.props_type.as_deref().map(|value| bounded_text(value, 512)),
                            }),
                            gaps: json!(["Defaults, spreads, runtime validation and serialization remain unchecked."]),
                        },
                    )?
                {
                    counts.component_properties += 1;
                }
                for state in component.states {
                    if !framework_kernel::component_hooks_compatible(effective_client, 1) {
                        counts.component_gaps += 1;
                    }
                    if self.component_detail(
                        rows, counts, &id, path,
                        DetailFact {
                            line: state.line, kind: "component-state", prefix: "frfcs1",
                            status: if effective_client { "candidate" } else { "conflict" },
                            confidence: "name-only",
                            basis: "react-state-hook", detail_name: "state",
                            detail: json!({"hook": state.hook, "binding": state.binding.as_deref().map(|name| bounded_text(name, 160)),
                                "setter": state.setter.as_deref().map(|name| bounded_text(name, 160))}),
                            gaps: if effective_client { json!(["Initial values and update behavior remain unchecked."]) }
                            else { json!(["A state hook appears without a captured use-client boundary."]) },
                        },
                    )? { counts.component_states += 1; }
                }
                for effect in component.effects {
                    if !framework_kernel::component_hooks_compatible(effective_client, 1) {
                        counts.component_gaps += 1;
                    }
                    if self.component_detail(
                        rows, counts, &id, path,
                        DetailFact {
                            line: effect.line, kind: "component-effect", prefix: "frfce1",
                            status: if effective_client { "candidate" } else { "conflict" },
                            confidence: "name-only",
                            basis: "react-effect-hook", detail_name: "effect",
                            detail: json!({"hook": effect.hook, "schedule": effect.schedule,
                                "dependency_count": effect.dependency_count,
                                "cleanup_candidate": effect.cleanup_candidate}),
                            gaps: if effective_client { json!(["Dependency identity, cleanup and runtime execution remain unchecked."]) }
                            else { json!(["An effect hook appears without a captured use-client boundary."]) },
                        },
                    )? { counts.component_effects += 1; }
                }
                for hook in component.hooks {
                    if self.component_detail(
                        rows,
                        counts,
                        &id,
                        path,
                        DetailFact {
                            line: hook.line,
                            kind: "component-hook",
                            prefix: "frfch1",
                            status: "unresolved",
                            confidence: "name-only",
                            basis: "react-unmodeled-hook-call",
                            detail_name: "hook",
                            detail: json!({"name": bounded_text(&hook.name, 160), "kind": hook.kind}),
                            gaps: json!(["The hook implementation, imported identity, state, effects and runtime requirements remain unchecked."]),
                        },
                    )? {
                        counts.component_hooks += 1;
                    }
                }
                for event in component.events {
                    if self.component_detail(
                        rows, counts, &id, path,
                        DetailFact { line: event.line, kind: "component-event", prefix: "frfcv1",
                            status: "candidate", confidence: "name-only", basis: "jsx-event-attribute", detail_name: "event",
                            detail: json!({"name": bounded_text(&event.name, 160), "element": event.element.as_deref().map(|name| bounded_text(name, 160)), "handler_kind": event.handler_kind}),
                            gaps: json!(["Event delegation, payload type and handler behavior remain unchecked."]) },
                    )? { counts.component_events += 1; }
                }
                for style in component.styles {
                    if self.component_detail(
                        rows, counts, &id, path,
                        DetailFact { line: style.line, kind: "component-style", prefix: "frfcy1",
                            status: "candidate", confidence: "name-only", basis: "jsx-style-attribute", detail_name: "style",
                            detail: json!({"attribute": style.attribute, "value_kind": style.value_kind}),
                            gaps: json!(["Class values, CSS resolution, cascade and runtime style output remain unchecked."]) },
                    )? { counts.component_styles += 1; }
                }
                for render in component.renders {
                    let (status, confidence, detail, gaps) = self.render_resolution(
                        feature,
                        path,
                        &declarations,
                        &imports,
                        &render.target,
                    )?;
                    if self.component_detail(
                        rows,
                        counts,
                        &id,
                        path,
                        DetailFact {
                            line: render.line,
                            kind: "component-render",
                            prefix: "frfcr1",
                            status,
                            confidence,
                            basis: "jsx-component-element",
                            detail_name: "render",
                            detail,
                            gaps,
                        },
                    )? {
                        counts.render_edges += 1;
                    }
                }
            }
        }
        Ok(())
    }
}
