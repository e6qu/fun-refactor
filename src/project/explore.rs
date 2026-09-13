use super::*;
use anyhow::ensure;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Mode {
    Names,
    Behavior,
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
    let mut arguments = vec![
        "project".to_owned(),
        "explore".to_owned(),
        options.term.clone(),
        "--mode".to_owned(),
        "behavior".to_owned(),
        "--target".to_owned(),
        target,
    ];
    if options.contains {
        arguments.push("--contains".to_owned());
    }
    if options.profile == AgentProfile::Expanded {
        push_option(&mut arguments, "--profile", "expanded");
    }
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
            options.target.is_none() && options.offset == 0 && options.relations_cursor.is_none(),
            "--target, --offset and --relations-cursor require --mode behavior."
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
        let (start, end, page) = page(matches.len(), limit, options.cursor.as_deref(), &key)?;
        let rows = matches[start..end]
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
                let arguments = behavior_arguments(options, self.handle(*id));
                json!({
                    "handle": self.handle(*id),
                    "kind": node.kind,
                    "name": bounded_text(&node.name, 160),
                    "path": bounded_text(&node.path.to_string_lossy(), 256),
                    "line": line,
                    "next": {"reason": "inspect-behavior", "arguments": arguments}
                })
            })
            .collect::<Vec<_>>();
        let mut continuations = Vec::new();
        if let Some(next) = page["next"].as_str() {
            let mut arguments = base_arguments(options, Mode::Names);
            push_option(&mut arguments, "--cursor", next);
            continuations.push(json!({"reason": "more-name-matches", "arguments": arguments}));
        }
        if options.profile == AgentProfile::Compact && matches.len() > limit {
            let mut arguments = base_arguments(options, Mode::Names);
            push_option(&mut arguments, "--profile", "expanded");
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
        result["rows"] = json!(rows);
        result["page"] = page;
        result["omitted"] = json!({"matching_locals": hidden_locals});
        result["continuations"] = json!(continuations);
        ensure!(
            serde_json::to_vec(&result)?.len() <= options.profile.report_bytes(),
            "agent discovery report exceeded its enforced profile budget; narrow --in."
        );
        Ok(result)
    }

    fn explore_behavior(&self, options: &Options) -> Result<Value> {
        ensure!(options.cursor.is_none(), "--cursor requires --mode names.");
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

        let mut declaration = self.show(&ShowOptions {
            handle: target.to_owned(),
            revision: None,
            source: true,
            offset: options.offset,
            bytes: options.profile.source_bytes(),
            relations: true,
            limit: options.profile.relationship_limit(),
            cursor: options.relations_cursor.clone(),
        })?;
        let source_next = declaration["source"]["next_offset"].as_u64();
        let relationships = declaration
            .as_object_mut()
            .and_then(|object| object.remove("relations"))
            .context("behavior inspection did not return relationships.")?;
        let relationships_next = relationships["page"]["next"].as_str().map(str::to_owned);
        strip_envelope(&mut declaration);

        let mut continuations = Vec::new();
        if let Some(offset) = source_next {
            let mut arguments = base_arguments(options, Mode::Behavior);
            push_option(&mut arguments, "--target", target);
            push_option(&mut arguments, "--offset", offset);
            continuations.push(json!({"reason": "more-source", "arguments": arguments}));
        }
        if let Some(cursor) = relationships_next {
            let mut arguments = base_arguments(options, Mode::Behavior);
            push_option(&mut arguments, "--target", target);
            push_option(&mut arguments, "--offset", options.offset);
            push_option(&mut arguments, "--relations-cursor", cursor);
            continuations.push(json!({"reason": "more-relationships", "arguments": arguments}));
        }
        if options.profile == AgentProfile::Compact
            && (!continuations.is_empty()
                || serde_json::to_vec(&declaration)?.len()
                    + serde_json::to_vec(&relationships)?.len()
                    > options.profile.report_bytes() / 2)
        {
            let mut arguments = base_arguments(options, Mode::Behavior);
            push_option(&mut arguments, "--target", target);
            push_option(&mut arguments, "--profile", "expanded");
            continuations
                .push(json!({"reason": "explicit-profile-expansion", "arguments": arguments}));
        }

        let mut result = self.envelope("explore");
        result["mode"] = json!(Mode::Behavior);
        result["profile"] = self.exploration_profile(options.profile);
        result["match"] = json!({
            "term": options.term,
            "mode": if options.contains { "literal-substring" } else { "exact" }
        });
        result["declaration"] = declaration;
        result["relationships"] = relationships;
        result["continuations"] = json!(continuations);
        ensure!(
            serde_json::to_vec(&result)?.len() <= options.profile.report_bytes(),
            "agent discovery report exceeded its enforced profile budget; use a smaller profile page."
        );
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
