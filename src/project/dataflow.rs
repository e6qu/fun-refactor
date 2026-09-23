//! Bounded, path-insensitive scalar value propagation for a declared Python subset.
use super::{hash, occurrence::Occurrence, Project};
use crate::{lang::Language, parse::Parsers, span::Span};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use tree_sitter::Node;

#[derive(Args)]
pub struct Options {
    /// Exact revision-bound function handle.
    target: String,
    /// Versioned source, sink, propagation and sanitizer contracts.
    #[arg(long)]
    rules: Option<std::path::PathBuf>,
    #[arg(long, default_value_t = 256)]
    steps: usize,
    #[arg(long, default_value_t = 8)]
    depth: usize,
    #[arg(long, default_value = "generic")]
    context: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub version: String,
    #[serde(default)]
    pub sources: BTreeSet<String>,
    #[serde(default)]
    pub sinks: BTreeSet<String>,
    #[serde(default)]
    pub propagators: BTreeSet<String>,
    /// Function name to the sole context in which its returned scalar removes origins.
    #[serde(default)]
    pub sanitizers: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Trace {
    origin: String,
    occurrences: Vec<Occurrence>,
}
type Flow = BTreeMap<String, Trace>;
type Environment = BTreeMap<String, Flow>;
#[derive(Clone)]
struct State {
    values: Environment,
    returned: Option<Flow>,
}

struct Analyzer<'a, 'p, 't> {
    project: &'a Project<'p>,
    file: &'a Path,
    source: &'a str,
    functions: BTreeMap<String, Node<'t>>,
    ambiguous: BTreeSet<String>,
    rules: Rules,
    context: &'a str,
    remaining: usize,
    depth: usize,
    active: Vec<String>,
    cutoffs: BTreeSet<String>,
    events: Vec<Value>,
    witnesses: Vec<Value>,
}

fn merge(into: &mut Flow, from: Flow) {
    for (origin, trace) in from {
        into.entry(origin).or_insert(trace);
    }
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    fn occurrence(&self, node: Node<'_>, role: &str) -> Occurrence {
        self.project
            .occurrence(self.file, Span::from(node), role)
            .expect("parsed node in selected snapshot")
    }
    fn text(&self, node: Node<'_>) -> &str {
        &self.source[node.byte_range()]
    }
    fn tick(&mut self, node: Node<'_>, kind: &str) -> bool {
        if self.remaining == 0 {
            self.cutoffs.insert("step-budget".into());
            return false;
        }
        self.remaining -= 1;
        self.events
            .push(json!({"kind": kind, "occurrence": self.occurrence(node, kind)}));
        true
    }
    fn cutoff(&mut self, reason: impl Into<String>) {
        self.cutoffs.insert(reason.into());
    }
    fn extend(&self, mut flow: Flow, node: Node<'_>, role: &str) -> Flow {
        let occurrence = self.occurrence(node, role);
        for trace in flow.values_mut() {
            if trace.occurrences.last() != Some(&occurrence) {
                trace.occurrences.push(occurrence.clone());
            }
        }
        flow
    }
    fn expression(&mut self, node: Node<'t>, env: &Environment) -> Flow {
        if !self.tick(node, "use") {
            return Flow::new();
        }
        match node.kind() {
            "integer" | "float" | "true" | "false" | "none" => Flow::new(),
            "string" => {
                if node
                    .named_children(&mut node.walk())
                    .any(|child| child.kind() == "interpolation")
                {
                    self.cutoff("string-interpolation");
                }
                Flow::new()
            }
            "identifier" => match env.get(self.text(node)) {
                Some(value) => self.extend(value.clone(), node, "use"),
                None => {
                    self.cutoff(format!("unknown-binding:{}", self.text(node)));
                    Flow::new()
                }
            },
            "call" => self.call(node, env),
            "binary_operator"
            | "unary_operator"
            | "boolean_operator"
            | "comparison_operator"
            | "parenthesized_expression" => {
                let mut flow = Flow::new();
                for child in node.named_children(&mut node.walk()) {
                    merge(&mut flow, self.expression(child, env));
                }
                self.extend(flow, node, "expression")
            }
            _ => {
                self.cutoff(format!("unsupported-expression:{}", node.kind()));
                Flow::new()
            }
        }
    }
    fn call(&mut self, node: Node<'t>, env: &Environment) -> Flow {
        let Some(function) = node.child_by_field_name("function") else {
            self.cutoff("missing-call-target");
            return Flow::new();
        };
        if function.kind() != "identifier" {
            self.cutoff("dynamic-or-attribute-call");
            return Flow::new();
        }
        let name = self.text(function).to_owned();
        let mut arguments = Vec::new();
        if let Some(args) = node.child_by_field_name("arguments") {
            for arg in args.named_children(&mut args.walk()) {
                arguments.push(self.expression(arg, env));
            }
        }
        let mut joined = Flow::new();
        for argument in &arguments {
            merge(&mut joined, argument.clone());
        }
        if env.contains_key(&name) || self.ambiguous.contains(&name) {
            self.cutoff(format!("ambiguous-call:{name}"));
            return joined;
        }
        if self.rules.sources.contains(&name) {
            let occurrence = self.occurrence(node, "source");
            let origin = format!("{}:{name}:{}", self.rules.version, occurrence.id);
            joined.insert(
                origin.clone(),
                Trace {
                    origin,
                    occurrences: vec![occurrence],
                },
            );
            return joined;
        }
        if self.rules.sinks.contains(&name) {
            let flow = self.extend(joined.clone(), node, "sink");
            for trace in flow.values() {
                self.witnesses.push(json!({"sink": name,
                "context": self.context, "rules": self.rules.version, "trace": trace,
                "claim": "possible-value-propagation; path feasibility unchecked"}));
            }
            return joined;
        }
        if let Some(context) = self.rules.sanitizers.get(&name) {
            return if context == self.context {
                Flow::new()
            } else {
                self.extend(joined, node, "sanitizer-context-mismatch")
            };
        }
        if self.rules.propagators.contains(&name) {
            return self.extend(joined, node, "propagation-rule");
        }
        let Some(function) = self.functions.get(&name).copied() else {
            self.cutoff(format!("unknown-external-call:{name}"));
            return joined;
        };
        if self.active.contains(&name) {
            self.cutoff(format!("recursion:{name}"));
            return joined;
        }
        if self.active.len() >= self.depth {
            self.cutoff("call-depth-budget");
            return joined;
        }
        self.invoke(&name, function, arguments)
    }
    fn invoke(&mut self, name: &str, function: Node<'t>, arguments: Vec<Flow>) -> Flow {
        if function
            .children(&mut function.walk())
            .any(|child| child.kind() == "async")
        {
            self.cutoff("async-function");
            return Flow::new();
        }
        let Some(parameters) = function.child_by_field_name("parameters") else {
            self.cutoff("missing-parameters");
            return Flow::new();
        };
        let parameters: Vec<_> = parameters.named_children(&mut parameters.walk()).collect();
        if parameters.len() != arguments.len()
            || parameters.iter().any(|p| p.kind() != "identifier")
        {
            self.cutoff(format!("unsupported-parameter-contract:{name}"));
            return Flow::new();
        }
        let values = parameters
            .into_iter()
            .zip(arguments)
            .map(|(p, value)| (self.text(p).to_owned(), self.extend(value, p, "parameter")))
            .collect();
        self.active.push(name.to_owned());
        let states = function
            .child_by_field_name("body")
            .map(|body| {
                self.block(
                    body,
                    vec![State {
                        values,
                        returned: None,
                    }],
                )
            })
            .unwrap_or_default();
        self.active.pop();
        let mut result = Flow::new();
        for state in states {
            if let Some(flow) = state.returned {
                merge(&mut result, flow);
            }
        }
        result
    }
    fn block(&mut self, block: Node<'t>, mut states: Vec<State>) -> Vec<State> {
        for statement in block.named_children(&mut block.walk()) {
            let mut next = Vec::new();
            for state in states {
                if state.returned.is_some() {
                    next.push(state);
                    continue;
                }
                next.extend(self.statement(statement, state));
                if next.len() > 128 {
                    self.cutoff("branch-budget");
                    next.truncate(128);
                    break;
                }
            }
            states = next;
            if self.remaining == 0 {
                break;
            }
        }
        states
    }
    fn statement(&mut self, node: Node<'t>, mut state: State) -> Vec<State> {
        if !self.tick(node, "control") {
            return vec![state];
        }
        match node.kind() {
            "expression_statement" => {
                for expression in node.named_children(&mut node.walk()) {
                    if expression.kind() == "assignment" {
                        let left = expression.child_by_field_name("left").unwrap();
                        let right = expression.child_by_field_name("right").unwrap();
                        let value = self.expression(right, &state.values);
                        if left.kind() == "identifier" {
                            state.values.insert(
                                self.text(left).to_owned(),
                                self.extend(value, left, "definition"),
                            );
                        } else {
                            self.cutoff("alias-or-destructuring-assignment");
                        }
                    } else {
                        self.expression(expression, &state.values);
                    }
                }
            }
            "return_statement" => {
                let mut returned = Flow::new();
                for expression in node.named_children(&mut node.walk()) {
                    merge(&mut returned, self.expression(expression, &state.values));
                }
                state.returned = Some(self.extend(returned, node, "return"));
            }
            "if_statement" => {
                if let Some(condition) = node.child_by_field_name("condition") {
                    self.expression(condition, &state.values);
                }
                let mut alternatives = node
                    .child_by_field_name("consequence")
                    .map(|body| self.block(body, vec![state.clone()]))
                    .unwrap_or_default();
                if let Some(other) = node.child_by_field_name("alternative") {
                    if other.kind() == "else_clause" {
                        if let Some(body) = other.child_by_field_name("body") {
                            alternatives.extend(self.block(body, vec![state]));
                        }
                    } else {
                        self.cutoff("elif-control");
                        alternatives.push(state);
                    }
                } else {
                    alternatives.push(state);
                }
                return alternatives;
            }
            "while_statement" | "for_statement" => {
                // Retain zero and one iteration witnesses. No fixed-point/safety claim.
                self.cutoff("loop-fixed-point-unchecked");
                let mut alternatives = vec![state.clone()];
                if let Some(condition) = node.child_by_field_name("condition") {
                    self.expression(condition, &state.values);
                }
                if let Some(body) = node.child_by_field_name("body") {
                    alternatives.extend(self.block(body, vec![state]));
                }
                return alternatives;
            }
            "pass_statement" | "comment" => (),
            _ => self.cutoff(format!("unsupported-statement:{}", node.kind())),
        }
        vec![state]
    }
}

impl Project<'_> {
    pub(super) fn dataflow(&self, options: &Options) -> Result<Value> {
        ensure!(
            (1..=4096).contains(&options.steps) && (1..=32).contains(&options.depth),
            "invalid analysis budget"
        );
        let selected = self.target(&options.target)?;
        let symbol = self.nodes[selected]
            .symbol
            .and_then(|id| self.index.symbol(id))
            .context("select a function")?;
        ensure!(
            symbol.language == Language::Python,
            "dataflow admits the Python scalar subset only"
        );
        let source = &self.sources[&symbol.file];
        let parsed = Parsers::new().parse(Language::Python, source)?;
        ensure!(!parsed.root().has_error(), "dataflow refuses syntax errors");
        let mut functions = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();
        let mut module_effects = false;
        for node in parsed.root().named_children(&mut parsed.root().walk()) {
            if node.kind() == "function_definition" {
                let name = &source[node.child_by_field_name("name").unwrap().byte_range()];
                if functions.insert(name.to_owned(), node).is_some() {
                    ambiguous.insert(name.to_owned());
                }
            } else if !matches!(node.kind(), "comment" | "expression_statement")
                || (node.kind() == "expression_statement"
                    && node.named_child(0).is_some_and(|n| n.kind() != "string"))
            {
                module_effects = true;
            }
        }
        let function = *functions
            .get(&symbol.name)
            .context("select a top-level scalar function")?;
        ensure!(
            Span::from(function) == symbol.full_span,
            "selected function does not match top-level definition"
        );
        let rules = if let Some(path) = &options.rules {
            ensure!(
                std::fs::metadata(path)?.len() <= 65536,
                "rule file exceeds 64 KiB"
            );
            let rules: Rules = serde_json::from_slice(&std::fs::read(path)?)?;
            ensure!(
                !rules.version.is_empty(),
                "rules require a version identity"
            );
            let mut names = BTreeSet::new();
            for name in rules
                .sources
                .iter()
                .chain(&rules.sinks)
                .chain(&rules.propagators)
                .chain(rules.sanitizers.keys())
            {
                ensure!(
                    names.insert(name) && !functions.contains_key(name),
                    "rule overlaps another rule or local definition"
                );
            }
            rules
        } else {
            Rules {
                version: "no-external-summaries-1".into(),
                ..Rules::default()
            }
        };
        let rules_digest = hash(&rules)?;
        let mut analyzer = Analyzer {
            project: self,
            file: &symbol.file,
            source,
            functions,
            ambiguous,
            rules,
            context: &options.context,
            remaining: options.steps,
            depth: options.depth,
            active: Vec::new(),
            cutoffs: BTreeSet::new(),
            events: Vec::new(),
            witnesses: Vec::new(),
        };
        if module_effects {
            analyzer.cutoff("module-effects-unchecked");
        }
        if analyzer.ambiguous.contains(&symbol.name) {
            analyzer.cutoff("ambiguous-entry");
        }
        let parameters = function
            .child_by_field_name("parameters")
            .context("missing parameters")?;
        let arguments = parameters
            .named_children(&mut parameters.walk())
            .map(|p| {
                let occurrence = analyzer.occurrence(p, "entry-parameter");
                let origin = format!("parameter:{}", analyzer.text(p));
                BTreeMap::from([(
                    origin.clone(),
                    Trace {
                        origin,
                        occurrences: vec![occurrence],
                    },
                )])
            })
            .collect();
        let returns = analyzer.invoke(&symbol.name, function, arguments);
        Ok(
            json!({"schema": "fr-dataflow-1", "revision": self.revision, "handle_prefix": format!("frp1:{}:", &self.revision[..32]), "coverage": self.coverage(), "target": options.target,
            "semantics": "python-scalar-explicit-values-1", "claim": "possible-value-propagation",
            "scope": "selected function and direct helpers in the same file",
            "complete": analyzer.cutoffs.is_empty(), "cutoffs": analyzer.cutoffs,
            "assumptions": ["scalar values; no aliases, monkey patching or implicit flows", "branch feasibility unchecked",
                "external rules are caller-supplied contracts", "exceptions and resource effects are outside this model"],
            "rules_digest": rules_digest, "context": options.context,
            "events": analyzer.events, "returns": returns, "witnesses": analyzer.witnesses,
            "budget": {"steps": options.steps, "used": options.steps - analyzer.remaining, "call_depth": options.depth}}),
        )
    }
}
