use super::Project;
use crate::capabilities::{support, Capability as C, Support};
use crate::edit::EditSet;
use crate::span::Span;
use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub(super) struct Plan {
    pub edits: EditSet,
    pub report: Value,
}
impl Project<'_> {
    pub(super) fn intent_capability(
        &self,
        target: &str,
        capability: C,
        parameters: &BTreeMap<String, String>,
        range: Option<Span>,
    ) -> Result<Plan> {
        let node = &self.nodes[self.resolve_handle(target)?];
        let file = self.root.join(&node.path);
        let language = self
            .index
            .file(&file)
            .map(|info| info.language)
            .context("direct capability requires one indexed file or declaration")?;
        ensure!(
            matches!(support(capability, language), Support::Yes),
            "capability does not admit the selected language."
        );
        let form = capability.agent_form();
        let known = ["handle", "path", "position", "language", "range"];
        let mut names = form
            .arguments
            .iter()
            .filter_map(|arg| arg.strip_prefix('{')?.strip_suffix('}'))
            .filter(|name| !known.contains(name))
            .collect::<BTreeSet<_>>();
        if capability == C::RemoveFlag {
            names.insert("value");
        }
        ensure!(
            parameters.keys().all(|key| names.contains(key.as_str())),
            "capability parameters must name public argument fields."
        );
        ensure!(
            parameters
                .values()
                .all(|value| !value.is_empty() && value.len() <= 4096 && !value.contains('\0')),
            "capability parameters must be bounded nonempty strings."
        );
        let parameter = |name: &str| {
            parameters
                .get(name)
                .map(String::as_str)
                .with_context(|| format!("capability requires {name}"))
        };
        let symbol = || {
            node.symbol
                .context("capability requires an exact declaration target")
        };
        let selected_range = || -> Result<Span> {
            let range = range.context("extraction requires a reviewed byte range")?;
            let source = self
                .sources
                .get(&file)
                .context("selected source is absent")?;
            ensure!(
                range.start < range.end
                    && range.end <= source.len()
                    && source.is_char_boundary(range.start)
                    && source.is_char_boundary(range.end),
                "capability range is invalid."
            );
            if let Some(id) = node.symbol {
                let span = self
                    .index
                    .symbol(id)
                    .context("selected symbol is absent")?
                    .full_span;
                ensure!(
                    range.start >= span.start && range.end <= span.end,
                    "capability range lies outside the intent declaration."
                );
            }
            Ok(range)
        };
        let offset = || -> Result<usize> {
            if range.is_some() {
                return Ok(selected_range()?.start);
            }
            self.index
                .symbol(symbol()?)
                .map(|symbol| symbol.name_span.start)
                .context("selected symbol is absent")
        };
        ensure!(
            range.is_none()
                || matches!(
                    capability,
                    C::ExtractVariable
                        | C::ExtractFunction
                        | C::InlineCall
                        | C::MicroRewrites
                        | C::Flow
                ),
            "this capability does not accept a range."
        );
        let mut result = Plan {
            edits: EditSet::new(),
            report: json!({"capability":capability,"target":target}),
        };
        match capability {
            C::Rename => {
                let plan =
                    crate::refactor::rename::plan(self.index, symbol()?, parameter("new_name")?)?;
                result.report["planner"] = json!({"old_name":plan.old_name,"new_name":plan.new_name,
                    "reference_edits":plan.reference_edits,"warnings":plan.warnings});
                result.edits = plan.edits;
            }
            C::SafeDelete => {
                let plan = crate::refactor::delete::plan(self.index, symbol()?)?;
                result.report["planner"] =
                    json!({"name":plan.name,"sites":plan.sites,"warnings":plan.warnings});
                result.edits = plan.edits;
            }
            C::OrganizeImports => {
                let plan = crate::refactor::imports::plan(self.index, &file)?;
                result.report["planner"] = json!({"warnings":plan.warnings});
                result.edits = plan.edits;
            }
            C::ChangeSignature => {
                let change = crate::refactor::signature::Change::parse(parameter("change")?)?;
                let plan = crate::refactor::signature::change(self.index, symbol()?, change)?;
                result.report["planner"] =
                    json!({"subject":plan.subject,"call_sites":plan.call_sites,"notes":plan.notes});
                result.edits = plan.edits;
            }
            C::InlineVariable => {
                let plan = crate::refactor::inline::variable(self.index, symbol()?)?;
                result.report["planner"] = json!({"name":plan.name,"use_sites":plan.use_sites});
                result.edits = plan.edits;
            }
            C::InlineCall => {
                let plan = crate::refactor::inline::call(self.index, &file, offset()?)?;
                result.report["planner"] = json!({"function":plan.function});
                result.edits = plan.edits;
            }
            C::ExtractVariable => {
                let plan = crate::refactor::extract::variable(
                    self.index,
                    &file,
                    selected_range()?,
                    parameter("name")?,
                    false,
                )?;
                result.report["planner"] = json!({"name":parameter("name")?});
                result.edits = plan.edits;
            }
            C::ExtractFunction => {
                let plan = crate::refactor::extract::function(
                    self.index,
                    &file,
                    selected_range()?,
                    parameter("name")?,
                )?;
                result.report["planner"] = json!({"name":parameter("name")?});
                result.edits = plan.edits;
            }
            C::MicroRewrites => {
                let rewrite = crate::refactor::rewrite::available(self.index, &file, offset()?)?
                    .into_iter()
                    .find(|rewrite| {
                        rewrite.as_str()
                            == parameters.get("rewrite").map(String::as_str).unwrap_or("")
                    })
                    .context("rewrite is not available at the exact selected declaration")?;
                result.edits =
                    crate::refactor::rewrite::apply(self.index, &file, offset()?, rewrite)?.edits;
                result.report["planner"] = json!({"rewrite":rewrite.as_str()});
            }
            C::MoveToFile => {
                let relative = PathBuf::from(parameter("destination")?);
                ensure!(
                    !relative.is_absolute()
                        && relative
                            .components()
                            .all(|part| matches!(part, std::path::Component::Normal(_))),
                    "move destination must be workspace-relative."
                );
                let destination = self.root.join(relative);
                let parent = destination
                    .parent()
                    .context("destination has no parent")?
                    .canonicalize()?;
                ensure!(
                    parent.starts_with(&self.root),
                    "move destination leaves the workspace."
                );
                let plan =
                    crate::refactor::move_symbol::to_file(self.index, symbol()?, &destination)?;
                result.report["planner"] =
                    json!({"warnings":plan.warnings,"imports_added":plan.imports_added});
                result.edits = plan.edits;
            }
            C::Restructure => {
                let plan = crate::refactor::restructure::apply(
                    self.index,
                    language,
                    parameter("pattern")?,
                    parameter("template")?,
                )?;
                result.report["planner"] = json!({"scope":"workspace-language","matches":plan.matches.len(),
                    "skipped_with_comments":plan.skipped_with_comments});
                result.edits = plan.edits;
            }
            C::RemoveFlag => {
                let value = parameters
                    .get("value")
                    .map(String::as_str)
                    .unwrap_or("true")
                    .parse::<bool>()
                    .context("flag value must be true or false")?;
                let sources = self
                    .scanned
                    .files
                    .iter()
                    .map(|file| {
                        (
                            file.path.clone(),
                            (file.language, self.sources[&file.path].clone()),
                        )
                    })
                    .collect();
                struct ResetVfs;
                impl Drop for ResetVfs {
                    fn drop(&mut self) {
                        crate::vfs::use_filesystem();
                    }
                }
                let reset = ResetVfs;
                let plan = crate::refactor::cascade::remove_flag_in_for(
                    sources,
                    &crate::refactor::cascade::FlagTarget::At(file.clone(), offset()?),
                    value,
                )?;
                drop(reset);
                result.report["planner"] = json!({"value":value});
                result.edits = plan.edits;
            }
            C::Translate => {
                let to = crate::lang::Language::from_name(parameter("target_language")?)
                    .context("unknown translation language")?;
                let plan = crate::translate::plan(&file, to)?;
                result.report["planner"] =
                    json!({"from":plan.from,"to":plan.to,"destination":plan.destination});
                result.edits = plan.edits;
            }
            C::DeclaredType => {
                result.report["planner"] =
                    serde_json::to_value(crate::analysis::types::of(self.index, symbol()?)?)?;
            }
            C::Impact => {
                let impact = crate::analysis::impact::analyse(self.index, symbol()?, 3)?;
                result.report["planner"] = json!({"callers_beyond_depth":impact.callers_beyond_the_depth_limit,
                    "items":impact.items.iter().map(|item|json!({"file":item.file.strip_prefix(&self.root).unwrap_or(&item.file),
                        "line":item.line,"col":item.col,"kind":item.kind.as_str(),"confidence":item.confidence.as_str(),"detail":item.detail})).collect::<Vec<_>>()});
            }
            C::CallGraph => {
                let graph = crate::analysis::call_graph::CallGraph::built(self.index);
                result.report["planner"] = json!({"callers":self.evidence_trace_for_intent(&graph,symbol()?,true,3),
                    "callees":self.evidence_trace_for_intent(&graph,symbol()?,false,3),
                    "hierarchy_gaps":graph.hierarchy_gaps});
            }
            C::Flow => {
                let flow = crate::analysis::flow::backward(self.index, &file, offset()?, 3)?;
                result.report["planner"] = self.evidence_flow_steps(&flow);
            }
            C::Provenance => {
                let trace = crate::analysis::provenance::provenance(self.index, symbol()?, 3)?;
                result.report["planner"] = json!({"hops":trace.hops.iter().map(|hop|json!({
                    "symbol":hop.symbol,"kind":hop.kind.as_str(),"file":hop.file.strip_prefix(&self.root).unwrap_or(&hop.file),
                    "span":hop.span,"line":hop.line,"depth":hop.depth,"confidence":hop.confidence.as_str()})).collect::<Vec<_>>(),
                    "competitions":trace.competitions.iter().map(|competition|json!({"subject":competition.subject,"model":competition.model,
                        "decided":competition.decided,"sources":competition.sources.iter().map(|source|json!({"symbol":source.hop.symbol,
                            "kind":source.hop.kind.as_str(),"precedence":source.precedence.label,"wins":source.wins})).collect::<Vec<_>>() })).collect::<Vec<_>>(),
                    "stops":trace.stops.iter().map(|(depth,reason)| {
                        use crate::analysis::provenance::StopReason;
                        let kind = match reason {
                            StopReason::Origin(_) => "origin",
                            StopReason::ExternalInput { .. } => "external-input",
                            StopReason::Unresolved(_) => "unresolved",
                            StopReason::DepthLimit => "depth-limit",
                            StopReason::RenderDependent(_) => "render-dependent",
                            StopReason::Conditional { .. } => "conditional",
                            StopReason::ComputedAtApply(_) => "computed-at-apply",
                            StopReason::PrecedenceUndetermined(_) => "precedence-undetermined",
                            StopReason::DecidedGivenInputs { .. } => "decided-given-inputs",
                            StopReason::NotAValue(_) => "not-a-value",
                        };
                        json!({"depth":depth,"kind":kind})
                    }).collect::<Vec<_>>()});
            }
            C::EntryPoints => {
                let entries = crate::analysis::entrypoints::Catalog::builtin()?.detect(self.index);
                result.report["scope"] = json!("workspace");
                result.report["planner"] = json!(entries
                    .iter()
                    .map(|entry| json!({
                    "symbol":entry.symbol,"kind":entry.kind.as_str(),"rule":entry.rule,
                    "threat_model":format!("{:?}",entry.threat_model)}))
                    .collect::<Vec<_>>());
            }
            C::DeadCode => {
                let entries = crate::analysis::entrypoints::Entrypoints::detect(self.index)?;
                let report = crate::refactor::delete::find_unused_report(self.index, &entries);
                result.report["planner"] = json!({"unused":report.unused.iter().filter(|id| self.index.symbol(**id).is_some_and(|symbol|symbol.file == file)).collect::<Vec<_>>(),
                    "spared":report.spared.iter().map(|(id,reason)|json!({"symbol":id,"reason":format!("{:?}",reason)})).collect::<Vec<_>>(),"hierarchy_gaps":report.hierarchy_gaps});
            }
            C::Duplicates => {
                result.report["planner"] =
                    serde_json::to_value(crate::analysis::duplicates::find(
                        self.index,
                        &crate::analysis::duplicates::Options {
                            languages: vec![language],
                            paths: vec![file],
                            ..Default::default()
                        },
                    )?)?;
            }
            C::Stitch => {
                result.report["scope"] = json!("workspace");
                let analysis =
                    crate::analysis::stitch::analyze_snapshot(self.index, &self.sources)?;
                result.report["planner"] = json!({"chains":analysis.chains.iter().map(|chain|json!({"env_var":chain.env_var,
                    "declared_in":chain.declared_in,"declared_line":chain.declared_line,
                    "values_path":chain.values_path,"values_file":chain.values_file,
                    "conditional_on":chain.conditional_on,"reads":chain.reads.iter().map(|read|json!({"file":read.file,"language":read.language})).collect::<Vec<_>>() })).collect::<Vec<_>>(),"gaps":analysis.gaps});
            }
            C::Symbols => {
                result.report["planner"] = self.evidence_model(self.resolve_handle(target)?, 0)?;
            }
            C::Openapi => {
                result.report["planner"] = self.batch_manifest(
                    super::batch::Manifest {
                        schema: super::batch::SCHEMA.into(),
                        requests: vec![super::batch::Request {
                            id: "contracts".into(),
                            arguments: ["contracts", target, "--limit", "8"]
                                .iter()
                                .map(|value| super::batch::Argument::Literal((*value).into()))
                                .collect(),
                        }],
                    },
                    65536,
                    "intent",
                )?;
            }
        }
        Ok(result)
    }
}
