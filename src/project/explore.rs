use super::*;
use anyhow::ensure;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Mode {
    Names,
    Behavior,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
enum View {
    Both,
    Source,
    Relationships,
}

impl View {
    fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Source => "source",
            Self::Relationships => "relationships",
        }
    }
}

#[derive(clap::Args)]
pub struct Options {
    #[arg(help = "Exact declaration name, or a case-sensitive substring with --contains.")]
    term: String,
    #[arg(long = "in", default_value = ".")]
    scope: String,
    #[arg(long, help = "Required when --in is a short node ID.")]
    revision: Option<String>,
    #[arg(long, help = "Match a case-sensitive literal substring.")]
    contains: bool,
    #[arg(long, value_enum, default_value = "names")]
    mode: Mode,
    #[arg(
        long,
        value_enum,
        default_value = "both",
        help = "Behavior view: combined, source only, or relationships only."
    )]
    view: View,
    #[arg(long, help = "Full handle selected by the names response.")]
    target: Option<String>,
    #[arg(
        long,
        default_value_t = 0,
        help = "Source byte offset for behavior mode."
    )]
    offset: usize,
    #[arg(long, help = "Relationship cursor returned by behavior mode.")]
    relations_cursor: Option<String>,
    #[arg(long, help = "Name-page cursor returned by names mode.")]
    cursor: Option<String>,
    #[arg(long, value_enum, default_value = "compact")]
    pub(super) profile: AgentProfile,
}

fn push_option(arguments: &mut Vec<String>, name: &str, value: impl ToString) {
    arguments.push(name.to_owned());
    arguments.push(value.to_string());
}

fn base_arguments(options: &Options, mode: Mode) -> Vec<String> {
    let mut arguments = vec![
        "project".to_owned(),
        "explore".to_owned(),
        options.term.clone(),
    ];
    if options.scope != "." {
        push_option(&mut arguments, "--in", &options.scope);
    }
    if let Some(revision) = &options.revision {
        push_option(&mut arguments, "--revision", revision);
    }
    if options.contains {
        arguments.push("--contains".to_owned());
    }
    if mode == Mode::Behavior {
        push_option(&mut arguments, "--mode", "behavior");
    }
    if options.profile == AgentProfile::Expanded {
        push_option(&mut arguments, "--profile", "expanded");
    }
    arguments
}

fn behavior_arguments(options: &Options, target: String) -> Vec<String> {
    let mut arguments = base_arguments(options, Mode::Behavior);
    push_option(&mut arguments, "--target", target);
    arguments
}

fn strip_envelope(report: &mut Value) {
    if let Some(object) = report.as_object_mut() {
        for field in ["schema", "revision", "handle_prefix", "coverage", "query"] {
            object.remove(field);
        }
    }
}

impl Project<'_> {
    pub(super) fn explore(&self, options: &Options) -> Result<Value> {
        ensure!(
            !options.term.is_empty() && options.term.len() <= 160,
            "Choose a nonempty literal term of at most 160 UTF-8 bytes."
        );
        ensure!(
            agent_discovery_transition_allowed(
                match options.mode {
                    Mode::Names => 0,
                    Mode::Behavior => 1,
                },
                options.target.is_some(),
            ),
            "names mode rejects --target; behavior mode requires it."
        );
        ensure!(
            agent_discovery_budget_admitted(
                options.profile.row_limit(),
                options.profile.source_bytes(),
                options.profile.report_bytes(),
                options.profile,
            ),
            "invalid built-in agent discovery budget."
        );
        match options.mode {
            Mode::Names => self.explore_names(options),
            Mode::Behavior => self.explore_behavior(options),
        }
    }

    fn explore_names(&self, options: &Options) -> Result<Value> {
        ensure!(
            options.target.is_none()
                && options.offset == 0
                && options.relations_cursor.is_none()
                && options.view == View::Both,
            "--target, --offset, --relations-cursor and focused --view require --mode behavior."
        );
        let selected =
            self.target(&self.explicit_handle(&options.scope, options.revision.as_deref())?)?;
        let mut stack = vec![selected];
        let mut matches = Vec::new();
        let mut hidden_locals = 0usize;
        while let Some(id) = stack.pop() {
            let node = &self.nodes[id];
            if node.symbol.is_some()
                && if options.contains {
                    node.name.contains(&options.term)
                } else {
                    node.name == options.term
                }
            {
                if self.local(id) {
                    hidden_locals += 1;
                } else {
                    matches.push(id);
                }
            }
            stack.extend(node.children.iter().rev());
        }
        let key = format!(
            "frpc1:{}",
            &hash((
                &self.revision,
                "explore-names",
                selected,
                &options.term,
                options.contains,
                options.profile,
            ))?[..32]
        );
        let limit = options.profile.row_limit();
        let (start, end, _) = page(matches.len(), limit, options.cursor.as_deref(), &key)?;
        let mut rows = matches[start..end]
            .iter()
            .map(|id| {
                let node = &self.nodes[*id];
                let symbol = node.symbol.and_then(|symbol| self.index.symbol(symbol));
                let line = self.source(*id).ok().map(|(source, span)| {
                    self.lines[&self.root.join(&node.path)]
                        .line_col(
                            symbol.map_or(span.start, |symbol| symbol.name_span.start),
                            source,
                        )
                        .line
                });
                let location = symbol.map(|symbol| self.definition_location(symbol));
                let arguments = behavior_arguments(options, self.handle(*id));
                let mut row = json!({
                    "handle": self.handle(*id),
                    "kind": node.kind,
                    "name": bounded_text(&node.name, 160),
                    "path": bounded_text(&node.path.to_string_lossy(), 256),
                    "line": line,
                    "location": location,
                    "next": {"reason": "inspect-behavior", "arguments": arguments}
                });
                if symbol.is_some_and(|symbol| {
                    symbol.language == crate::lang::Language::Python
                        && symbol.kind == crate::model::SymbolKind::Function
                        && symbol.is_top_level()
                }) {
                    row["analysis"] = json!({"arguments":["project","flow-facts",self.handle(*id),
                        "--limit","8","--steps","1024","--bytes","32768"],
                        "scope":"Python scalar model; external contracts require explicit --rules.",
                        "reference":"skills/fr/references/investigations.md"});
                }
                row
            })
            .collect::<Vec<_>>();
        loop {
            let (_, _, page) = page(
                matches.len(),
                rows.len().max(1),
                options.cursor.as_deref(),
                &key,
            )?;
            let mut continuations = Vec::new();
            if let Some(next) = page["next"].as_str() {
                let mut arguments = base_arguments(options, Mode::Names);
                push_option(&mut arguments, "--cursor", next);
                continuations.push(json!({"reason": "more-name-matches", "arguments": arguments}));
            }
            if options.profile == AgentProfile::Compact
                && page["remaining"].as_u64().unwrap_or(0) > 0
            {
                let mut arguments = base_arguments(options, Mode::Names);
                push_option(&mut arguments, "--profile", "expanded");
                if start != 0 {
                    let expanded_key = format!(
                        "frpc1:{}",
                        &hash((
                            &self.revision,
                            "explore-names",
                            selected,
                            &options.term,
                            options.contains,
                            AgentProfile::Expanded
                        ))?[..32]
                    );
                    push_option(
                        &mut arguments,
                        "--cursor",
                        format!("{expanded_key}:{start}"),
                    );
                }
                continuations
                    .push(json!({"reason": "explicit-profile-expansion", "arguments": arguments}));
            }
            let mut result = self.envelope("explore");
            result["mode"] = json!(Mode::Names);
            result["profile"] = self.exploration_profile(options.profile);
            result["match"] = json!({
                "term": options.term,
                "mode": if options.contains { "literal-substring" } else { "exact" }
            });
            result["root"] = json!(self.handle(selected));
            result["rows"] = json!(&rows);
            result["page"] = page;
            result["omitted"] = json!({"matching_locals": hidden_locals});
            result["continuations"] = json!(continuations);
            if self.exploration_fits(&result, options.profile)? {
                return Ok(result);
            }
            ensure!(
                rows.len() > 1,
                "one discovery row exceeds the profile budget; use a shorter --in handle."
            );
            rows.pop();
        }
    }

    fn explore_behavior(&self, options: &Options) -> Result<Value> {
        ensure!(options.cursor.is_none(), "--cursor requires --mode names.");
        ensure!(
            options.view != View::Source || options.relations_cursor.is_none(),
            "--relations-cursor is unavailable with --view source."
        );
        ensure!(
            options.view != View::Relationships || options.offset == 0,
            "--offset is unavailable with --view relationships."
        );
        let target = options
            .target
            .as_deref()
            .context("--mode behavior requires --target from a names response.")?;
        ensure!(
            target.starts_with("frp1:"),
            "--mode behavior requires a full revision-bound project handle."
        );
        let id = self.resolve_handle(target)?;
        let selected =
            self.target(&self.explicit_handle(&options.scope, options.revision.as_deref())?)?;
        ensure!(
            self.within(id, selected),
            "selected behavior target is outside --in scope."
        );
        let node = &self.nodes[id];
        ensure!(
            node.symbol.is_some(),
            "selected behavior target is not a declaration."
        );
        ensure!(
            if options.contains {
                node.name.contains(&options.term)
            } else {
                node.name == options.term
            },
            "selected behavior target does not match the discovery term."
        );

        let mut source_bytes = options.profile.source_bytes();
        let mut relationship_limit = options.profile.relationship_limit();
        loop {
            let result = self.explore_behavior_page(options, source_bytes, relationship_limit)?;
            if self.exploration_fits(&result, options.profile)? {
                return Ok(result);
            }
            let source = &result["declaration"]["source"];
            let relationships = &result["relationships"];
            let source_length = source["returned_bytes"].as_u64().unwrap_or(0) as usize;
            let relation_count = relationships["items"].as_array().map_or(0, Vec::len);
            let shrink_source = source_length > 4;
            let shrink_relations = relation_count > 1;
            ensure!(shrink_source || shrink_relations,
                "discovery metadata exceeds the profile budget; select a focused --view or shorter --in handle.");
            if shrink_source
                && (!shrink_relations
                    || serde_json::to_vec(source)?.len()
                        >= serde_json::to_vec(relationships)?.len())
            {
                source_bytes = (source_length / 2).max(4);
            } else {
                relationship_limit = (relation_count / 2).max(1);
            }
        }
    }

    fn exploration_fits(&self, report: &Value, profile: AgentProfile) -> Result<bool> {
        let mut delivered = report.clone();
        self.response_context(None)?.apply(&mut delivered)?;
        Ok(serde_json::to_vec(&delivered)?.len() + 1 <= profile.report_bytes())
    }

    fn explore_behavior_page(
        &self,
        options: &Options,
        source_bytes: usize,
        relationship_limit: usize,
    ) -> Result<Value> {
        let mut declaration = self.show(&ShowOptions {
            handle: options.target.as_ref().unwrap().to_owned(),
            revision: None,
            source: options.view != View::Relationships,
            offset: options.offset,
            bytes: source_bytes,
            relations: options.view != View::Source,
            limit: relationship_limit,
            cursor: options.relations_cursor.clone(),
        })?;
        let source_next = declaration["source"]["next_offset"].as_u64();
        let relationships = declaration
            .as_object_mut()
            .context("behavior inspection did not return a declaration.")?
            .remove("relations");
        let relationships_next = relationships
            .as_ref()
            .and_then(|report| report["page"]["next"].as_str());
        strip_envelope(&mut declaration);

        let mut continuations = Vec::new();
        if let Some(offset) = source_next {
            let mut arguments =
                behavior_arguments(options, options.target.as_ref().unwrap().to_owned());
            push_option(&mut arguments, "--view", "source");
            push_option(&mut arguments, "--offset", offset);
            continuations.push(json!({"reason": "more-source", "arguments": arguments}));
        }
        if let Some(cursor) = relationships_next {
            let mut arguments =
                behavior_arguments(options, options.target.as_ref().unwrap().to_owned());
            push_option(&mut arguments, "--view", "relationships");
            push_option(&mut arguments, "--relations-cursor", cursor);
            continuations.push(json!({"reason": "more-relationships", "arguments": arguments}));
        }
        if options.profile == AgentProfile::Compact
            && (!continuations.is_empty()
                || serde_json::to_vec(&declaration)?.len()
                    + serde_json::to_vec(&relationships)?.len()
                    > options.profile.report_bytes() / 2)
        {
            let mut arguments =
                behavior_arguments(options, options.target.as_ref().unwrap().to_owned());
            push_option(&mut arguments, "--view", options.view.as_str());
            push_option(&mut arguments, "--profile", "expanded");
            if options.offset != 0 {
                push_option(&mut arguments, "--offset", options.offset);
            }
            if let Some(cursor) = &options.relations_cursor {
                push_option(&mut arguments, "--relations-cursor", cursor);
            }
            continuations
                .push(json!({"reason": "explicit-profile-expansion", "arguments": arguments}));
        }

        let mut result = self.envelope("explore");
        result["mode"] = json!(Mode::Behavior);
        result["view"] = json!(options.view);
        result["profile"] = self.exploration_profile(options.profile);
        result["match"] = json!({
            "term": options.term,
            "mode": if options.contains { "literal-substring" } else { "exact" }
        });
        result["declaration"] = declaration;
        if let Some(relationships) = relationships {
            result["relationships"] = relationships;
        }
        result["continuations"] = json!(continuations);
        Ok(result)
    }

    fn exploration_profile(&self, profile: AgentProfile) -> Value {
        json!({
            "name": profile,
            "row_limit": profile.row_limit(),
            "relationship_limit": profile.relationship_limit(),
            "source_bytes": profile.source_bytes(),
            "report_bytes": profile.report_bytes(),
            "enforcement": "server"
        })
    }
}
