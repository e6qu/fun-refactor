use super::{
    control_flow::{Graph, Operation},
    hash,
    occurrence::Occurrence,
    Project,
};
use crate::{
    lang::Language,
    parse::{Parsed, Parsers},
    span::Span,
};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

#[path = "flow_assignments.rs"]
mod assignments;

#[path = "flow_calls.rs"]
mod calls;

#[path = "flow_standard_library.rs"]
mod standard_library;

#[path = "flow_initializers.rs"]
mod initializers;

pub(super) fn call_contract() -> Value {
    calls::contract()
}

#[path = "flow_expressions.rs"]
mod expressions;

#[path = "flow_effects.rs"]
mod effects;

#[path = "flow_completion.rs"]
mod completion;

pub(super) fn completion_contract() -> Value {
    completion::contract()
}

#[path = "flow_text.rs"]
mod text_values;

pub(super) fn expression_contract() -> Value {
    expressions::contract()
}

pub(super) fn effect_contract() -> Value {
    json!({"exception_safety_established":false,"shared_mutable_values_supported":false,
        "complete_means":"The scalar model finished; it does not check runtime exception safety or shared mutable objects."})
}

#[path = "flow_modules.rs"]
mod modules;

#[path = "flow_startup.rs"]
mod saved_startup;

pub(super) fn value_bindings_valid(value: &Value) -> bool {
    modules::value_bindings_valid(value)
}

pub(super) fn standard_imports_valid(value: &Value) -> bool {
    standard_library::imports_valid(value)
}
#[path = "flow_summaries.rs"]
mod summaries;

#[derive(Clone, Default, Serialize)]
struct Initialization {
    sites: Vec<InitializationSite>,
    omitted: usize,
    by_file: BTreeMap<String, InitializationCounts>,
}

#[derive(Clone, Default, Serialize)]
struct InitializationCounts {
    statements: usize,
    reasons: BTreeMap<&'static str, usize>,
}

#[derive(Clone, Serialize)]
struct InitializationSite {
    occurrence: Occurrence,
    reasons: Vec<&'static str>,
}

enum ParallelValue {
    Literal(bool),
    Builtin,
    Function(String, usize),
    Value(bool, usize),
}

impl Initialization {
    fn record(
        &mut self,
        project: &Project<'_>,
        file: &Path,
        node: Node<'_>,
        reasons: Vec<&'static str>,
    ) -> Result<()> {
        if reasons.is_empty() {
            return Ok(());
        }
        let path = file
            .strip_prefix(&project.root)?
            .to_string_lossy()
            .into_owned();
        let counts = self.by_file.entry(path).or_default();
        counts.statements += 1;
        for reason in &reasons {
            *counts.reasons.entry(reason).or_default() += 1;
        }
        if self.sites.len() == 16 {
            self.omitted += 1;
        } else {
            self.sites.push(InitializationSite {
                occurrence: project.occurrence(file, Span::from(node), "module-initialization")?,
                reasons,
            });
        }
        Ok(())
    }
}

struct Startup<'a, 't> {
    functions: BTreeMap<String, Node<'t>>,
    function_aliases: BTreeMap<String, (String, usize)>,
    captured_defaults: BTreeMap<(PathBuf, usize), calls::CapturedDefaults>,
    contexts: BTreeMap<String, (&'a Path, &'a str)>,
    ambiguous: BTreeSet<String>,
    inert_bindings: BTreeSet<String>,
    initialization: Initialization,
    dictionary_statements: Vec<Occurrence>,
    dictionary_omitted: usize,
}

#[derive(Clone, Args)]
pub struct Options {
    target: String,
    #[arg(long, requires = "summaries", conflicts_with_all = ["inputs_only", "reuse"],
          help = "Ask another function in the same file using one shared startup check (16 total).")]
    also: Vec<String>,
    #[arg(
        long,
        help = "Solve symbolic function summaries, including same-file recursion."
    )]
    summaries: bool,
    #[arg(
        long,
        requires = "summaries",
        help = "Follow static workspace-local Python modules and regular packages."
    )]
    imports: bool,
    #[arg(
        long,
        requires = "imports",
        value_delimiter = ',',
        help = "Select explicit standard-library module models; local candidates must be absent."
    )]
    standard_library: Vec<String>,
    #[arg(long)]
    rules: Option<std::path::PathBuf>,
    #[arg(long, default_value_t = 256)]
    steps: usize,
    #[arg(long, default_value_t = 8)]
    depth: usize,
    #[arg(long, default_value = "generic")]
    context: String,
    #[arg(long, default_value_t = 65536)]
    bytes: usize,
    #[arg(long, conflicts_with = "reuse")]
    inputs_only: bool,
    #[arg(long, requires = "reuse_digest")]
    reuse: Option<std::path::PathBuf>,
    #[arg(long, requires = "reuse")]
    reuse_digest: Option<String>,
    #[arg(long, requires = "summaries", conflicts_with_all = ["also", "inputs_only", "reuse"],
          help = "Return the answer and checked startup work for later questions.")]
    with_startup: bool,
    #[arg(long, requires_all = ["with_startup", "startup_digest"])]
    reuse_startup: Option<PathBuf>,
    #[arg(long, requires = "reuse_startup")]
    startup_digest: Option<String>,
}

impl Options {
    pub(super) fn for_facts(
        target: &str,
        rules: Option<std::path::PathBuf>,
        context: &str,
        steps: usize,
        depth: usize,
        imports: bool,
    ) -> Self {
        Self {
            target: target.into(),
            also: Vec::new(),
            summaries: true,
            imports,
            standard_library: Vec::new(),
            rules,
            steps,
            depth,
            context: context.into(),
            bytes: 1_048_576,
            inputs_only: false,
            reuse: None,
            reuse_digest: None,
            with_startup: false,
            reuse_startup: None,
            startup_digest: None,
        }
    }
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
    #[serde(default)]
    pub sanitizers: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Trace {
    origin: String,
    occurrences: Vec<Occurrence>,
}
type Flow = BTreeMap<String, Trace>;
#[derive(Clone)]
struct PendingReturn {
    flow: Flow,
    plain_string: bool,
}

#[derive(Clone)]
struct Environment {
    values: BTreeMap<String, Flow>,
    defined: BTreeSet<String>,
    plain_strings: BTreeSet<String>,
    pending_returns: BTreeMap<usize, PendingReturn>,
    pending_raises: BTreeMap<usize, Flow>,
}

fn join_environment(into: &mut Environment, from: &Environment) -> bool {
    let mut changed = false;
    for (name, flow) in &from.values {
        if !into.values.contains_key(name) {
            changed = true;
        }
        let target = into.values.entry(name.clone()).or_default();
        for (origin, trace) in flow {
            if !target.contains_key(origin) {
                target.insert(origin.clone(), trace.clone());
                changed = true;
            }
        }
    }
    let defined = into.defined.intersection(&from.defined).cloned().collect();
    if into.defined != defined {
        into.defined = defined;
        changed = true;
    }
    let strings = into
        .plain_strings
        .intersection(&from.plain_strings)
        .cloned()
        .collect();
    if into.plain_strings != strings {
        into.plain_strings = strings;
        changed = true;
    }
    changed |= completion::join_pending(into, from);
    changed
}

struct Analyzer<'a, 'p, 't> {
    project: &'a Project<'p>,
    file: &'a Path,
    source: &'a str,
    functions: BTreeMap<String, Node<'t>>,
    function_aliases: BTreeMap<String, (String, usize)>,
    captured_defaults: BTreeMap<(PathBuf, usize), calls::CapturedDefaults>,
    ambiguous: BTreeSet<String>,
    inert_bindings: BTreeSet<String>,
    modules: Option<&'a modules::Modules>,
    contexts: BTreeMap<String, (&'a Path, &'a str)>,
    rules: Rules,
    context: &'a str,
    remaining: usize,
    depth: usize,
    active: Vec<String>,
    local_bindings: Vec<BTreeSet<String>>,
    cutoffs: BTreeSet<String>,
    effect_limits: BTreeMap<(String, usize, usize, String), Value>,
    events: Vec<Value>,
    witnesses: BTreeMap<String, Value>,
    graphs: BTreeMap<String, Value>,
    summaries: Vec<Value>,
    exceptional_returns: Flow,
    current_block: Option<(String, usize)>,
    origins: BTreeMap<String, Occurrence>,
    solver: summaries::Solver,
    normal: bool,
    normal_return: bool,
    return_text: Option<bool>,
    may_raise: bool,
    sink_flows: BTreeMap<String, summaries::SinkFlow>,
}

pub fn local_module_admitted(source: bool, package_missing: bool, stub_missing: bool) -> bool {
    source && package_missing && stub_missing
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
            .push(json!({"kind": kind, "occurrence": self.occurrence(node, kind), "control_node":self.current_block}));
        true
    }
    fn cutoff(&mut self, reason: impl Into<String>) {
        self.cutoffs.insert(reason.into());
    }
    fn extend(&self, mut flow: Flow, node: Node<'_>, role: &str) -> Flow {
        let occurrence = self.occurrence(node, role);
        for trace in flow.values_mut() {
            if !trace
                .occurrences
                .iter()
                .any(|previous| previous.id == occurrence.id)
            {
                trace.occurrences.push(occurrence.clone());
            }
        }
        flow
    }
    fn expression(&mut self, node: Node<'t>, env: &Environment) -> Flow {
        if self.solver.enabled && !self.normal {
            return Flow::new();
        }
        if self.solver.enabled
            && matches!(
                node.kind(),
                "boolean_operator"
                    | "conditional_expression"
                    | "comparison_operator"
                    | "not_operator"
                    | "parenthesized_expression"
            )
        {
            return self.controlled_expression(node, env);
        }
        if !self.tick(node, "use") {
            return Flow::new();
        }
        self.expression_effect_limit(node);
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
            "identifier" => match env.values.get(self.text(node)) {
                Some(value) => {
                    if !env.defined.contains(self.text(node)) {
                        self.cutoff(format!("possibly-unbound:{}", self.text(node)));
                    }
                    self.extend(value.clone(), node, "use")
                }
                None => {
                    self.cutoff(format!("unknown-binding:{}", self.text(node)));
                    Flow::new()
                }
            },
            "call" => self.call(node, env),
            "binary_operator"
                if self
                    .modules
                    .is_some_and(|modules| modules.standard_enabled("builtins"))
                    && node
                        .child_by_field_name("operator")
                        .is_some_and(|operator| self.text(operator) == "%")
                    && node.child_by_field_name("left").is_some_and(|left| {
                        left.kind() == "string" && calls::literal_default(left, self.source)
                    })
                    && node
                        .child_by_field_name("right")
                        .and_then(calls::unparenthesized)
                        .is_some_and(|right| right.kind() == "tuple") =>
            {
                let mut flow = self.expression(node.child_by_field_name("left").unwrap(), env);
                let right =
                    calls::unparenthesized(node.child_by_field_name("right").unwrap()).unwrap();
                for child in right
                    .named_children(&mut right.walk())
                    .filter(|node| node.kind() != "comment")
                {
                    merge(&mut flow, self.expression(child, env));
                }
                self.extend(flow, node, "standard-format-result")
            }
            "binary_operator"
            | "unary_operator"
            | "boolean_operator"
            | "comparison_operator"
            | "parenthesized_expression" => {
                let mut flow = Flow::new();
                for child in node
                    .named_children(&mut node.walk())
                    .filter(|child| !child.is_extra())
                {
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
        if let Some(value) = self.string_method(node, env) {
            return value;
        }
        let Some(function) = node.child_by_field_name("function") else {
            self.cutoff("missing-call-target");
            return Flow::new();
        };
        let name: String = self
            .text(function)
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if function.kind() != "identifier"
            && !(self.modules.is_some()
                && function.kind() == "attribute"
                && name.split('.').all(|part| {
                    !part.is_empty()
                        && part.chars().enumerate().all(|(i, c)| {
                            c == '_' || c.is_ascii_alphabetic() || i > 0 && c.is_ascii_digit()
                        })
                }))
        {
            self.effect_limit(node, "dynamic-call",
                "A call through an object or computed function may read or change shared state; this analysis cannot determine its effects.");
            self.cutoff("dynamic-or-attribute-call");
            return Flow::new();
        }
        let (arguments, argument_names) = self.call_arguments(node, env);
        let mut joined = Flow::new();
        if self.solver.enabled && !self.normal {
            return Flow::new();
        }
        for argument in &arguments {
            merge(&mut joined, argument.clone());
        }
        let base = name.split('.').next().unwrap();
        let resolved = self.modules.map_or_else(
            || {
                self.function_aliases
                    .get(&name)
                    .map_or_else(|| name.clone(), |(target, _)| target.clone())
            },
            |modules| modules.resolve(self.file, &name),
        );
        if env.values.contains_key(base)
            || self.ambiguous.contains(&resolved)
            || self
                .local_bindings
                .last()
                .is_some_and(|names| names.contains(base))
        {
            if function.kind() == "attribute" {
                self.effect_limit(node, "dynamic-call",
                    "A call through a local object may read or change shared state; this analysis cannot determine its effects.");
            }
            self.cutoff(format!("ambiguous-call:{name}"));
            return joined;
        }
        if argument_names.iter().any(Option::is_some)
            && (self.rules.sources.contains(&name)
                || self.rules.sinks.contains(&name)
                || self.rules.sanitizers.contains_key(&name)
                || self.rules.propagators.contains(&name))
        {
            self.cutoff(format!("keyword-rule-contract-unchecked:{name}"));
            return joined;
        }
        if self.rules.sources.contains(&name) {
            let occurrence = self.occurrence(node, "source");
            let origin = format!("{}:{name}:{}", self.rules.version, occurrence.id);
            self.origins.insert(origin.clone(), occurrence.clone());
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
            if self.solver.enabled {
                self.record_sink(&name, self.occurrence(node, "sink"), flow);
                return joined;
            }
            for trace in flow.values() {
                let key = format!("{:020}:{}", node.start_byte(), trace.origin);
                let site = self.occurrence(node, "sink");
                self.witnesses.entry(key).or_insert_with(|| {
                    json!({"sink": name,
                "context": self.context, "rules": self.rules.version, "trace": trace, "site":site,
                "claim": "possible-value-propagation; path feasibility unchecked."})
                });
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
        if let Some(standard) = self.standard_call_name(function, env) {
            if let Some((minimum, maximum)) = standard_library::arity(&standard) {
                let exception = matches!(
                    standard.as_str(),
                    "builtins.ValueError" | "builtins.TypeError"
                );
                if argument_names.iter().any(Option::is_some)
                    || !(minimum..=maximum).contains(&arguments.len())
                    || exception
                        && node
                            .parent()
                            .is_none_or(|parent| parent.kind() != "raise_statement")
                {
                    self.cutoff(format!("unsupported-standard-call:{standard}"));
                    return joined;
                }
                return self.extend(joined, node, "standard-call-result");
            }
        }
        let Some(function) = self.functions.get(&resolved).copied() else {
            self.cutoff(format!("unknown-external-call:{name}"));
            return joined;
        };
        if self.solver.enabled {
            let Some(arguments) =
                self.bind_arguments(&resolved, function, arguments, &argument_names)
            else {
                return Flow::new();
            };
            return self.summary_call(&resolved, function, arguments, node);
        }
        if self.active.contains(&name) {
            self.cutoff(format!("recursion:{name}"));
            return joined;
        }
        if self.active.len() >= self.depth {
            self.cutoff("call-depth-budget");
            return joined;
        }
        let result = self.invoke(&name, function, arguments);
        self.extend(result, node, "call-result")
    }
    fn invoke(&mut self, name: &str, function: Node<'t>, arguments: Vec<Flow>) -> Flow {
        if function.child_by_field_name("return_type").is_some()
            && !self.annotations_postponed(self.file)
        {
            self.cutoff("function-annotation-effects");
        }
        if function
            .children(&mut function.walk())
            .any(|child| child.kind() == "async")
        {
            self.cutoff("async-function");
            return Flow::new();
        }
        if calls::generator_function(function) {
            self.cutoff("generator-function");
            return Flow::new();
        }
        let Some(parameters) = self.parameter_nodes(function) else {
            self.cutoff(format!("unsupported-parameter-contract:{name}"));
            return Flow::new();
        };
        if parameters.len() != arguments.len() {
            self.cutoff(format!("unsupported-parameter-contract:{name}"));
            return Flow::new();
        }
        let Ok(graph) = Graph::lower(function) else {
            self.cutoff("control-flow-block-budget");
            return Flow::new();
        };
        self.graphs
            .entry(name.to_owned())
            .or_insert_with(|| graph.report(self.project, self.file, name));
        let inputs: Vec<_> = arguments
            .iter()
            .map(|flow| flow.keys().cloned().collect::<Vec<_>>())
            .collect();
        let values: BTreeMap<_, _> = parameters
            .into_iter()
            .zip(arguments)
            .map(|(p, value)| (self.text(p).to_owned(), self.extend(value, p, "parameter")))
            .collect();
        let initial = Environment {
            defined: values.keys().cloned().collect(),
            values,
            plain_strings: BTreeSet::new(),
            pending_returns: BTreeMap::new(),
            pending_raises: BTreeMap::new(),
        };
        let mut locals = initial.defined.clone();
        for block in &graph.blocks {
            if block.operation == Operation::Statement
                && block.syntax.is_some_and(|n| {
                    !matches!(
                        n.kind(),
                        "expression_statement" | "pass_statement" | "comment"
                    )
                })
            {
                self.cutoff("unsupported-lexical-binding");
            }
        }
        let mut lexical = function
            .child_by_field_name("body")
            .into_iter()
            .collect::<Vec<_>>();
        while let Some(node) = lexical.pop() {
            if node.kind() == "identifier" && !unicode_normalization::is_nfkc(self.text(node)) {
                self.cutoff("identifier-normalization-unchecked");
            }
            if self.solver.enabled
                && node.kind() == "call"
                && !calls::valid_call_syntax(node, self.source)
            {
                self.cutoff("unsupported-call-syntax");
            }
            if matches!(
                node.kind(),
                "assignment" | "augmented_assignment" | "named_expression"
            ) {
                if let Some(left) = node
                    .child_by_field_name("left")
                    .or_else(|| node.child_by_field_name("name"))
                {
                    if self.solver.enabled {
                        if let Some(names) = assignments::target_names(left) {
                            locals.extend(names.into_iter().map(|name| self.text(name).to_owned()));
                        } else {
                            self.cutoff("unsupported-local-binding");
                        }
                    } else if left.kind() == "identifier" {
                        locals.insert(self.text(left).to_owned());
                    } else {
                        self.cutoff("unsupported-local-binding");
                    }
                }
            }
            if !matches!(
                node.kind(),
                "function_definition" | "class_definition" | "lambda"
            ) {
                lexical.extend(node.named_children(&mut node.walk()));
            }
        }
        let mut incoming = vec![None; graph.blocks.len()];
        incoming[graph.entry] = Some(initial);
        let mut queue = VecDeque::from([graph.entry]);
        let mut queued = BTreeSet::from([graph.entry]);
        let mut visits = 0;
        let mut result = Flow::new();
        let parent_exceptions = std::mem::take(&mut self.exceptional_returns);
        let parent_may_raise = self.may_raise;
        let mut exceptions = Flow::new();
        let mut may_raise = false;
        let mut normal_return = false;
        let mut return_text = None;
        let parent_block = self.current_block.clone();
        self.active.push(name.to_owned());
        self.local_bindings.push(locals);
        while let Some(id) = queue.pop_front() {
            queued.remove(&id);
            if self.remaining == 0 {
                self.cutoff("step-budget");
                queue.push_front(id);
                break;
            }
            visits += 1;
            let mut state = incoming[id]
                .clone()
                .expect("queued blocks have input states");
            let block = &graph.blocks[id];
            let before = state.clone();
            self.normal = true;
            self.may_raise = false;
            self.exceptional_returns.clear();
            if block.operation == Operation::Exit {
                normal_return = true;
                if let Some(value) = state.pending_returns.get(&0) {
                    merge(&mut result, value.flow.clone());
                    return_text = Some(return_text.is_none_or(|prior| prior) && value.plain_string);
                } else {
                    return_text = Some(false);
                }
            }
            if block.operation == Operation::ExceptionalExit {
                may_raise = true;
                if let Some(value) = state.pending_raises.get(&0) {
                    merge(&mut exceptions, value.clone());
                }
            }
            if matches!(
                block.operation,
                Operation::ResumeReturn | Operation::ResumeRaise
            ) {
                let (source, destination) = block.completion.expect("resume completion slots");
                if block.operation == Operation::ResumeReturn {
                    if let Some(value) = state.pending_returns.get(&source).cloned() {
                        state.pending_returns.insert(destination, value);
                    } else {
                        self.cutoff("missing-return-completion");
                        self.normal = false;
                    }
                } else if let Some(value) = state.pending_raises.get(&source).cloned() {
                    state.pending_raises.insert(destination, value);
                } else {
                    self.cutoff("missing-raise-completion");
                    self.normal = false;
                }
            }
            self.current_block = Some((name.to_owned(), id));
            if let Some(node) = block.syntax {
                if !self.tick(node, "control") {
                    break;
                }
                match block.operation {
                    Operation::Condition => {
                        self.expression(node, &state);
                    }
                    Operation::Statement => self.statement(node, &mut state),
                    Operation::Return | Operation::Raise => {
                        if block.operation == Operation::Raise
                            && node.named_child_count() != 1
                        {
                            self.cutoff("raise-contract-unchecked");
                        }
                        let mut value = Flow::new();
                        for expression in node.named_children(&mut node.walk()) {
                            merge(&mut value, self.expression(expression, &state));
                        }
                        let value = self.extend(
                            value,
                            node,
                            if block.operation == Operation::Return {
                                "return"
                            } else {
                                "raise"
                            },
                        );
                        let slot = block.completion.expect("explicit completion slot").0;
                        if self.normal && block.operation == Operation::Return {
                            let plain = node.named_child_count() == 1
                                && node
                                    .named_child(0)
                                    .is_some_and(|value| self.plain_string(value, &state));
                            state.pending_returns.insert(
                                slot,
                                PendingReturn {
                                    flow: value,
                                    plain_string: plain,
                                },
                            );
                        } else if self.normal {
                            if !self.solver.enabled && self.active.len() > 1 {
                                self.cutoff("helper-exception-control-unchecked");
                            }
                            state.pending_raises.insert(slot, value);
                        }
                    }
                    _ => (),
                }
            }
            let mut successors = Vec::new();
            if self.may_raise {
                let mut raised_state = before;
                raised_state.pending_raises.insert(
                    block.raised.slot,
                    std::mem::take(&mut self.exceptional_returns),
                );
                successors.push((block.raised.node, raised_state));
            }
            if !self.solver.enabled || self.normal {
                for (successor, edge) in &block.successors {
                    let mut outgoing = state.clone();
                    if *successor == 0 && *edge != "return" {
                        outgoing.pending_returns.insert(
                            0,
                            PendingReturn {
                                flow: Flow::new(),
                                plain_string: false,
                            },
                        );
                    }
                    successors.push((*successor, outgoing));
                }
            }
            for (successor, outgoing) in successors {
                let changed = match &mut incoming[successor] {
                    Some(previous) => join_environment(previous, &outgoing),
                    slot @ None => {
                        *slot = Some(outgoing);
                        true
                    }
                };
                if changed && queued.insert(successor) {
                    queue.push_back(successor);
                }
            }
        }
        self.active.pop();
        self.local_bindings.pop();
        self.current_block = parent_block;
        self.exceptional_returns = parent_exceptions;
        merge(&mut self.exceptional_returns, exceptions);
        self.may_raise = parent_may_raise || may_raise;
        self.normal_return = normal_return;
        self.return_text = return_text;
        self.normal = normal_return;
        self.summaries.push(json!({"function":name,"inputs":inputs,"return_origins":result.keys().collect::<Vec<_>>(),
            "block_visits":visits,"converged":queue.is_empty(),"claim":if self.solver.enabled {"one symbolic summary evaluation; global convergence reported separately"} else {"context-specific evaluation; recursive summaries unsupported"}}));
        result
    }

    fn statement(&mut self, node: Node<'t>, state: &mut Environment) {
        if node.kind() == "try_statement" {
            self.effect_limit(node, "exception-handler",
                "Exception matching, handler clauses and combined try/except/finally statements are outside this scalar control-flow model.");
        }
        if node.kind() == "with_statement" {
            self.effect_limit(node, "resource-lifetime",
                "Entering and leaving a context can call code and change exception handling; those effects are unchecked.");
        }
        match node.kind() {
            "expression_statement" => {
                for expression in node.named_children(&mut node.walk()) {
                    if matches!(expression.kind(), "assignment" | "augmented_assignment") {
                        if let Some(target) = expression.child_by_field_name("left") {
                            if matches!(target.kind(), "subscript" | "attribute") {
                                self.effect_limit(target, "shared-write",
                                    "Writing an item or attribute can change the object seen through another name; this scalar analysis does not track that mutation.");
                            }
                        }
                    }
                    if expression.kind() == "assignment" && self.solver.enabled {
                        self.scalar_assignment(expression, state);
                    } else if expression.kind() == "assignment" {
                        let (Some(left), Some(right)) = (
                            expression.child_by_field_name("left"),
                            expression.child_by_field_name("right"),
                        ) else {
                            self.cutoff("unsupported-assignment");
                            continue;
                        };
                        let plain = self.plain_string(right, state);
                        let value = self.expression(right, state);
                        if left.kind() == "identifier" {
                            let name = self.text(left).to_owned();
                            state
                                .values
                                .insert(name.clone(), self.extend(value, left, "definition"));
                            state.defined.insert(name);
                            let name = self.text(left).to_owned();
                            if plain {
                                state.plain_strings.insert(name);
                            } else {
                                state.plain_strings.remove(&name);
                            }
                        } else {
                            self.cutoff("alias-or-destructuring-assignment");
                        }
                    } else {
                        self.expression(expression, state);
                    }
                }
            }
            "pass_statement" | "comment" => (),
            _ => self.cutoff(format!("unsupported-statement:{}", node.kind())),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DependencyQuery {
    path: String,
    function: String,
    rules: Option<String>,
    context: String,
    steps: usize,
    depth: usize,
    bytes: usize,
    standard_library: Vec<String>,
}

impl Project<'_> {
    pub(super) fn flow_dependency(&self, key: &str) -> Result<String> {
        let query: DependencyQuery = serde_json::from_str(key)?;
        let path = Path::new(&query.path);
        ensure!(
            !path.is_absolute()
                && !query.path.contains('\\')
                && query
                    .path
                    .split('/')
                    .all(|part| !matches!(part, "" | "." | ".."))
                && path.extension().is_some_and(|extension| extension == "py"),
            "flow dependency needs a normalized workspace-relative Python file."
        );
        let file = self.root.join(path);
        let symbols: Vec<_> = self
            .index
            .symbols
            .iter()
            .filter(|symbol| symbol.file == file && symbol.name == query.function)
            .collect();
        ensure!(
            symbols.len() == 1,
            "flow dependency requires one exact declaration."
        );
        let target = self.handle(
            *self
                .symbol_nodes
                .get(&symbols[0].id)
                .context("flow declaration is absent")?,
        );
        let rules = query
            .rules
            .map(|path| {
                let path = Path::new(&path);
                ensure!(
                    !path.is_absolute()
                        && path
                            .components()
                            .all(|part| matches!(part, std::path::Component::Normal(_))),
                    "flow rule dependency must stay inside the workspace."
                );
                Ok(self.root.join(path))
            })
            .transpose()?;
        let report = self.dataflow(&Options {
            target,
            also: Vec::new(),
            summaries: true,
            imports: true,
            standard_library: query.standard_library,
            rules,
            context: query.context,
            steps: query.steps,
            depth: query.depth,
            bytes: query.bytes,
            inputs_only: true,
            reuse: None,
            reuse_digest: None,
            with_startup: false,
            reuse_startup: None,
            startup_digest: None,
        })?;
        Ok(report["input_digest"]
            .as_str()
            .context("flow input identity is absent")?
            .into())
    }
}

impl Project<'_> {
    pub(super) fn dataflow(&self, options: &Options) -> Result<Value> {
        ensure!(
            (1..=4096).contains(&options.steps) && (1..=32).contains(&options.depth),
            "invalid analysis budget"
        );
        ensure!(
            (2048..=1_048_576).contains(&options.bytes),
            "invalid response byte budget"
        );
        let selected = self.target(&options.target)?;
        let symbol = self.nodes[selected]
            .symbol
            .and_then(|id| self.index.symbol(id))
            .context("select a function")?;
        ensure!(
            symbol.language == Language::Python,
            "dataflow admits the Python scalar subset only."
        );
        ensure!(
            options.also.len() < 16,
            "at most 16 questions may share startup."
        );
        ensure!(
            options.also.is_empty() || options.summaries,
            "shared startup requires summaries."
        );
        let mut targets = BTreeSet::from([options.target.as_str()]);
        for target in &options.also {
            ensure!(
                targets.insert(target),
                "shared startup requires distinct functions."
            );
            let selected = self.target(target)?;
            let other = self.nodes[selected]
                .symbol
                .and_then(|id| self.index.symbol(id))
                .context("select a function")?;
            ensure!(
                other.file == symbol.file && other.language == Language::Python,
                "shared startup requires functions in the same Python file."
            );
        }
        let source = &self.sources[&symbol.file];
        let parsed = Parsers::new().parse(Language::Python, source)?;
        ensure!(!parsed.has_errors(), "dataflow refuses syntax errors");
        let modules = options
            .imports
            .then(|| modules::Modules::load(self, &symbol.file, &options.standard_library))
            .transpose()?;
        let basis = options
            .with_startup
            .then(|| self.startup_inputs(&symbol.file, modules.as_ref()))
            .transpose()?;
        let retained = options
            .reuse_startup
            .as_ref()
            .map(|path| {
                self.restore_startup(
                    path,
                    options.startup_digest.as_deref().unwrap(),
                    basis.as_ref().unwrap(),
                    &symbol.file,
                    &parsed,
                    modules.as_ref(),
                )
            })
            .transpose()?
            .flatten();
        let reused = retained.is_some();
        let startup = match retained {
            Some(startup) => startup,
            None => {
                self.prepare_flow_startup(&symbol.file, source, &parsed, modules.as_ref(), options)?
            }
        };
        if options.with_startup {
            let answer = self.flow_answer(options, modules.as_ref(), &startup, options.steps)?;
            let record = if answer["complete"] == true
                && startup.ambiguous.is_empty()
                && startup.initialization.by_file.is_empty()
            {
                Some(self.save_startup(basis.unwrap(), &startup)?)
            } else {
                None
            };
            let report = json!({"schema":"fr-dataflow-prepared", "answer":answer,
                "revision":self.revision, "handle_prefix":format!("frp1:{}:", &self.revision[..32]),
                "coverage":self.coverage(),
                "startup":record, "preparation":{"kind":if reused {"retained"} else {"checked"},
                    "checks":usize::from(!reused), "import_checks":usize::from(options.imports),
                    "reason":if reused {"matching-inputs"} else if options.reuse_startup.is_some() {
                        "changed-inputs"} else {"no-saved-startup"},
                    "trust":"caller-supplied digest binds saved work; no execution attestation"}});
            ensure!(
                serde_json::to_vec(&report)?.len() + 512 <= options.bytes,
                "answer and saved startup exceed byte budget."
            );
            return Ok(report);
        }
        if options.also.is_empty() {
            return self.flow_answer(options, modules.as_ref(), &startup, options.steps);
        }
        let mut remaining = options.steps;
        let mut answers = Vec::new();
        for target in std::iter::once(&options.target).chain(&options.also) {
            let mut question = options.clone();
            question.target = target.clone();
            question.also.clear();
            let answer = self.flow_answer(&question, modules.as_ref(), &startup, remaining)?;
            remaining -= answer["execution"]["analysis_steps"]
                .as_u64()
                .context("missing flow work count.")? as usize;
            answers.push(answer);
        }
        let mut dependencies = answers[0]["inputs"].clone();
        dependencies.as_object_mut().unwrap().remove("selection");
        for answer in &answers[1..] {
            let mut current = answer["inputs"].clone();
            current.as_object_mut().unwrap().remove("selection");
            ensure!(
                current == dependencies,
                "shared startup inputs changed between questions."
            );
        }
        let report = json!({
            "schema":"fr-dataflow-together", "revision":self.revision,
            "handle_prefix":format!("frp1:{}:",&self.revision[..32]), "coverage":self.coverage(),
            "startup":{
                "checks":1, "questions":answers.len(),
                "input_digest":hash(&dependencies)?, "inputs":dependencies,
                "names":startup.functions.keys().chain(startup.inert_bindings.iter()).collect::<BTreeSet<_>>(),
                "dictionary_statements":startup.dictionary_statements,
                "dictionary_omitted":startup.dictionary_omitted,
            },
            "budget":{"steps":options.steps,"used":options.steps-remaining,"response_bytes":options.bytes},
            "answers":answers,
        });
        ensure!(
            serde_json::to_vec(&report)?.len() + 512 <= options.bytes,
            "combined response exceeds byte budget."
        );
        Ok(report)
    }

    fn prepare_flow_startup<'a, 't>(
        &'a self,
        file: &'a Path,
        source: &'a str,
        parsed: &'t Parsed,
        modules: Option<&'t modules::Modules>,
        options: &Options,
    ) -> Result<Startup<'a, 't>>
    where
        't: 'a,
    {
        let summary_mode = options.summaries;
        let import_mode = options.imports;
        let mut functions: BTreeMap<String, Node<'_>> = BTreeMap::new();
        let mut function_aliases: BTreeMap<String, (String, usize)> = BTreeMap::new();
        let mut value_alias_count = 0;
        let mut captured_defaults = BTreeMap::new();
        let mut contexts = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();
        let mut inert_bindings = BTreeSet::new();
        let mut literal_bindings = BTreeSet::new();
        let mut initialization = Initialization::default();
        let mut dictionary_statements = Vec::new();
        let mut dictionary_omitted = 0;
        let units: Vec<_> = if let Some(modules) = &modules {
            modules
                .parsed
                .iter()
                .map(|(file, parsed)| (file.as_path(), self.sources[file].as_str(), parsed))
                .collect()
        } else {
            vec![(file, source, parsed)]
        };
        for (file, text, parsed) in units {
            let postponed = modules
                .as_ref()
                .is_some_and(|m| m.annotations_postponed(file));
            let mut classes = BTreeSet::new();
            let mut documentable = BTreeSet::new();
            let mut default_values = BTreeSet::new();
            let mut default_scalars = BTreeSet::new();
            let mut value_alias_depths = BTreeMap::new();
            let explicit_builtins = calls::explicit_builtins(parsed.root(), text);
            let mut builtin_defaults = !explicit_builtins;
            let mut allocations = initializers::Allocations::default();
            let module_aliases: BTreeSet<_> = modules
                .as_ref()
                .and_then(|modules| modules.function_aliases.get(file))
                .into_iter()
                .flat_map(|aliases| aliases.values().map(String::as_str))
                .chain(
                    modules
                        .as_ref()
                        .and_then(|modules| modules.value_aliases.get(file))
                        .into_iter()
                        .flat_map(|aliases| aliases.values().map(String::as_str)),
                )
                .collect();
            let mut bound_names: BTreeSet<_> = modules
                .as_ref()
                .and_then(|modules| modules.bindings.get(file))
                .into_iter()
                .flat_map(|bindings| bindings.keys().map(String::as_str))
                .filter(|name| !module_aliases.contains(name))
                .chain(
                    modules
                        .into_iter()
                        .flat_map(|modules| modules.standard_names(file)),
                )
                .collect();
            let statements = modules.map_or_else(
                || {
                    parsed
                        .root()
                        .named_children(&mut parsed.root().walk())
                        .collect()
                },
                |modules| modules.statement_nodes(file, parsed),
            );
            for node in statements {
                let allocation = summary_mode
                    .then(|| {
                        allocations.statement(node, text, |name| {
                            explicit_builtins || bound_names.contains(name)
                        })
                    })
                    .flatten();
                if matches!(allocation, Some(initializers::Statement::Population)) {
                    if !options.also.is_empty() || options.with_startup {
                        if dictionary_statements.len() < 16 {
                            dictionary_statements.push(self.occurrence(
                                file,
                                Span::from(node),
                                "checked-dictionary-startup",
                            )?);
                        } else {
                            dictionary_omitted += 1;
                        }
                    }
                    continue;
                }
                let allocated = match allocation {
                    Some(initializers::Statement::Declaration(name)) => Some(name),
                    _ => None,
                };
                if summary_mode {
                    if let Some(pairs) = calls::parallel_assignment(node, text) {
                        let values: Option<Vec<_>> = pairs
                            .iter()
                            .map(|(name, value)| {
                                let offset = name.start_byte();
                                let name = &text[name.byte_range()];
                                if bound_names.contains(name) {
                                    return None;
                                }
                                if calls::collection_value(*value, text, false) {
                                    return Some(ParallelValue::Literal(calls::literal_default(
                                        *value, text,
                                    )));
                                }
                                if value.kind() != "identifier" {
                                    return None;
                                }
                                let target = &text[value.byte_range()];
                                if builtin_defaults
                                    && !bound_names.contains(target)
                                    && calls::builtin_reference(target)
                                {
                                    return Some(ParallelValue::Builtin);
                                }
                                let alias = if let Some(modules) = &modules {
                                    modules
                                        .function_aliases
                                        .get(file)
                                        .and_then(|aliases| aliases.get(&offset))
                                        .map(|_| (modules.resolve(file, name), 0))
                                } else {
                                    function_aliases.get(target).cloned().or_else(|| {
                                        functions
                                            .get(target)
                                            .filter(|_| documentable.contains(target))
                                            .map(|_| (target.to_owned(), 0))
                                    })
                                };
                                alias
                                    .filter(|(_, depth)| *depth < 15)
                                    .map(|(target, depth)| {
                                        ParallelValue::Function(target, depth + 1)
                                    })
                                    .or_else(|| {
                                        if let Some(modules) = &modules {
                                            modules.value_aliases.get(file)?.get(&offset)?;
                                            Some(ParallelValue::Value(
                                                modules.is_scalar_value(file, name),
                                                0,
                                            ))
                                        } else {
                                            let depth = value_alias_depths
                                                .get(target)
                                                .copied()
                                                .unwrap_or(0);
                                            (default_values.contains(target)
                                                && !functions.contains_key(target)
                                                && !function_aliases.contains_key(target)
                                                && depth < 15)
                                                .then(|| {
                                                    ParallelValue::Value(
                                                        default_scalars.contains(target),
                                                        depth + 1,
                                                    )
                                                })
                                        }
                                    })
                            })
                            .collect();
                        if let Some(values) = values.filter(|values| {
                            modules.is_some()
                                || function_aliases.len()
                                    + values
                                        .iter()
                                        .filter(|value| {
                                            matches!(
                                                value,
                                                ParallelValue::Function(..)
                                                    | ParallelValue::Value(..)
                                            )
                                        })
                                        .count()
                                    + value_alias_count
                                    <= 128
                        }) {
                            for ((name, _), value) in pairs.into_iter().zip(values) {
                                let name = &text[name.byte_range()];
                                bound_names.insert(name);
                                default_values.insert(name.to_owned());
                                let binding = modules.as_ref().map_or_else(
                                    || name.to_owned(),
                                    |modules| modules.function_name(file, name),
                                );
                                if let ParallelValue::Function(target, depth) = value {
                                    if modules.is_none() {
                                        function_aliases.insert(name.to_owned(), (target, depth));
                                    }
                                } else {
                                    literal_bindings.insert(binding.clone());
                                    if matches!(
                                        value,
                                        ParallelValue::Literal(true)
                                            | ParallelValue::Value(true, _)
                                    ) {
                                        default_scalars.insert(name.to_owned());
                                    }
                                    if let ParallelValue::Value(_, depth) = value {
                                        value_alias_depths.insert(name, depth);
                                        if modules.is_none() {
                                            value_alias_count += 1;
                                        }
                                    }
                                }
                                inert_bindings.insert(binding);
                            }
                            continue;
                        }
                    }
                    if calls::module_function_doc(node, text)
                        .is_some_and(|name| documentable.contains(name))
                    {
                        continue;
                    }
                    if let Some((name, _)) = calls::module_builtin_reference(node, text)
                        .filter(|(_, target)| !explicit_builtins && !bound_names.contains(target))
                    {
                        classes.remove(name);
                        documentable.remove(name);
                        default_values.remove(name);
                        default_scalars.remove(name);
                        if !bound_names.insert(name) {
                            initialization.record(self, file, node, vec!["binding-collision"])?;
                        } else {
                            default_values.insert(name.to_owned());
                        }
                        let binding = if let Some(modules) = &modules {
                            modules.function_name(file, name)
                        } else {
                            name.to_owned()
                        };
                        literal_bindings.insert(binding.clone());
                        inert_bindings.insert(binding);
                        continue;
                    }
                    if let Some((name, target)) = calls::module_function_alias(node, text) {
                        let alias = if let Some(modules) = &modules {
                            modules
                                .function_aliases
                                .get(file)
                                .and_then(|aliases| aliases.get(&node.start_byte()))
                                .map(|_| (modules.resolve(file, name), 0))
                        } else {
                            function_aliases.get(target).cloned().or_else(|| {
                                functions
                                    .get(target)
                                    .filter(|_| documentable.contains(target))
                                    .map(|_| (target.to_owned(), 0))
                            })
                        };
                        if let Some((target, depth)) = alias.filter(|(_, depth)| {
                            *depth < 15
                                && (modules.is_some()
                                    || function_aliases.len() + value_alias_count < 128)
                        }) {
                            classes.remove(name);
                            documentable.remove(name);
                            default_values.remove(name);
                            default_scalars.remove(name);
                            if !bound_names.insert(name) {
                                initialization.record(
                                    self,
                                    file,
                                    node,
                                    vec!["binding-collision"],
                                )?;
                            } else {
                                default_values.insert(name.to_owned());
                            }
                            let binding = if let Some(modules) = &modules {
                                modules.function_name(file, name)
                            } else {
                                function_aliases.insert(name.to_owned(), (target, depth + 1));
                                name.to_owned()
                            };
                            inert_bindings.insert(binding);
                            continue;
                        }
                        let depth = value_alias_depths.get(target).copied().unwrap_or(0);
                        let value = modules.as_ref().map_or_else(
                            || {
                                default_values.contains(target)
                                    && !functions.contains_key(target)
                                    && !function_aliases.contains_key(target)
                                    && depth < 15
                                    && function_aliases.len() + value_alias_count < 128
                            },
                            |modules| {
                                modules
                                    .value_aliases
                                    .get(file)
                                    .is_some_and(|aliases| aliases.contains_key(&node.start_byte()))
                            },
                        );
                        if value {
                            let scalar = modules.as_ref().map_or_else(
                                || default_scalars.contains(target),
                                |modules| modules.is_scalar_value(file, name),
                            );
                            classes.remove(name);
                            documentable.remove(name);
                            default_values.remove(name);
                            default_scalars.remove(name);
                            if !bound_names.insert(name) {
                                initialization.record(
                                    self,
                                    file,
                                    node,
                                    vec!["binding-collision"],
                                )?;
                            } else {
                                default_values.insert(name.to_owned());
                                if scalar {
                                    default_scalars.insert(name.to_owned());
                                }
                                value_alias_depths.insert(name, depth + 1);
                                if modules.is_none() {
                                    value_alias_count += 1;
                                }
                            }
                            let binding = modules.as_ref().map_or_else(
                                || name.to_owned(),
                                |modules| modules.function_name(file, name),
                            );
                            literal_bindings.insert(binding.clone());
                            inert_bindings.insert(binding);
                            continue;
                        }
                    }
                    if let Some((name, constructible)) =
                        calls::module_class(node, text, postponed, |name| {
                            explicit_builtins || bound_names.contains(name)
                        })
                    {
                        classes.remove(name);
                        documentable.remove(name);
                        default_scalars.remove(name);
                        if !bound_names.insert(name) {
                            default_values.remove(name);
                            initialization.record(self, file, node, vec!["binding-collision"])?;
                        } else {
                            default_values.insert(name.to_owned());
                            if constructible {
                                classes.insert(name);
                            }
                        }
                        inert_bindings.insert(if let Some(modules) = &modules {
                            modules.function_name(file, name)
                        } else {
                            name.to_owned()
                        });
                        continue;
                    }
                    let literal = allocated
                        .or_else(|| calls::module_literal(node, text))
                        .or_else(|| calls::module_collection(node, text))
                        .or_else(|| {
                            (!explicit_builtins)
                                .then(|| {
                                    calls::module_type_registry(node, text, |name| {
                                        bound_names.contains(name)
                                    })
                                })
                                .flatten()
                        })
                        .or_else(|| calls::module_metadata(node, text))
                        .or_else(|| calls::module_export_list(node, text).map(|_| "__all__"));
                    if let Some(name) =
                        literal.or_else(|| calls::module_instance(node, text, &classes))
                    {
                        classes.remove(name);
                        documentable.remove(name);
                        default_scalars.remove(name);
                        if !bound_names.insert(name) {
                            default_values.remove(name);
                            initialization.record(self, file, node, vec!["binding-collision"])?;
                        } else if !(name.starts_with("__") && name.ends_with("__")) {
                            default_values.insert(name.to_owned());
                            if calls::module_literal(node, text).is_some() {
                                default_scalars.insert(name.to_owned());
                            }
                        }
                        let binding = if let Some(modules) = &modules {
                            modules.function_name(file, name)
                        } else {
                            name.to_owned()
                        };
                        if literal.is_some() {
                            literal_bindings.insert(binding.clone());
                        }
                        inert_bindings.insert(binding);
                        continue;
                    }
                }
                if node.kind() == "function_definition" {
                    let mut reasons = Vec::new();
                    let mut definition_values = default_values.clone();
                    let mut definition_scalars = default_scalars.clone();
                    if modules.is_some_and(|modules| modules.standard_enabled("builtins")) {
                        let numeric = calls::numeric_defaults(node, text, |name| {
                            !builtin_defaults || bound_names.contains(name)
                        });
                        definition_values.extend(numeric.iter().cloned());
                        definition_scalars.extend(numeric);
                    }
                    definition_values.extend(calls::unshadowed_builtin_defaults(
                        node,
                        text,
                        |name| !builtin_defaults || bound_names.contains(name),
                    ));
                    if if summary_mode {
                        !calls::definition_is_inert_with_values(
                            node,
                            text,
                            postponed,
                            &definition_values,
                        )
                    } else {
                        node.child_by_field_name("return_type").is_some()
                            || node
                                .child_by_field_name("parameters")
                                .is_some_and(|parameters| {
                                    parameters
                                        .named_children(&mut parameters.walk())
                                        .any(|p| p.kind() != "identifier")
                                })
                    } {
                        reasons.push("definition-time-code");
                    }
                    let Some(name_node) = node.child_by_field_name("name") else {
                        continue;
                    };
                    let short = &text[name_node.byte_range()];
                    captured_defaults.insert(
                        (file.to_path_buf(), node.start_byte()),
                        calls::captured_defaults(
                            node,
                            text,
                            postponed,
                            &definition_values,
                            &definition_scalars,
                        ),
                    );
                    classes.remove(short);
                    documentable.remove(short);
                    default_values.remove(short);
                    default_scalars.remove(short);
                    if !bound_names.insert(short) {
                        reasons.push("binding-collision");
                    }
                    if !unicode_normalization::is_nfkc(short) {
                        reasons.push("non-normalized-identifier");
                    }
                    if reasons.is_empty() {
                        documentable.insert(short);
                        if short.is_ascii() && !(short.starts_with("__") && short.ends_with("__")) {
                            default_values.insert(short.to_owned());
                        }
                    } else {
                        documentable.clear();
                        default_values.clear();
                        default_scalars.clear();
                        builtin_defaults = false;
                    }
                    initialization.record(self, file, node, reasons)?;
                    let name = if let Some(modules) = &modules {
                        modules.function_name(file, short)
                    } else {
                        short.to_owned()
                    };
                    if functions.insert(name.clone(), node).is_some() {
                        ambiguous.insert(name.clone());
                    }
                    contexts.insert(name, (file, text));
                } else if !(import_mode
                    && matches!(node.kind(), "import_statement" | "import_from_statement"))
                    && !modules
                        .as_ref()
                        .is_some_and(|m| m.future_statement_is_inert(file, node.start_byte()))
                    && (!matches!(
                        node.kind(),
                        "comment" | "expression_statement" | "pass_statement"
                    ) || (node.kind() == "expression_statement"
                        && node.named_child(0).is_some_and(|n| {
                            n.kind() != "string"
                                || n.named_children(&mut n.walk())
                                    .any(|child| child.kind() == "interpolation")
                        })))
                {
                    documentable.clear();
                    default_values.clear();
                    default_scalars.clear();
                    builtin_defaults = false;
                    initialization.record(
                        self,
                        file,
                        node,
                        vec![initializers::unchecked_reason(node)],
                    )?;
                } else if matches!(node.kind(), "import_statement" | "import_from_statement") {
                    documentable.clear();
                    if let Some(modules) = &modules {
                        for name in modules.names_imported_at(file, node.start_byte()) {
                            default_values.remove(name);
                            default_scalars.remove(name);
                            if (modules.is_value(file, name) || modules.is_function(file, name))
                                && !(name.starts_with("__") && name.ends_with("__"))
                            {
                                default_values.insert(name.to_owned());
                                if modules.is_scalar_value(file, name) {
                                    default_scalars.insert(name.to_owned());
                                }
                            }
                        }
                    }
                }
            }
        }
        ambiguous.extend(
            literal_bindings
                .iter()
                .filter(|name| functions.contains_key(*name))
                .cloned(),
        );
        Ok(Startup {
            functions,
            function_aliases,
            captured_defaults,
            contexts,
            ambiguous,
            inert_bindings,
            initialization,
            dictionary_statements,
            dictionary_omitted,
        })
    }

    fn flow_answer(
        &self,
        options: &Options,
        modules: Option<&modules::Modules>,
        startup: &Startup<'_, '_>,
        allowance: usize,
    ) -> Result<Value> {
        let selected = self.target(&options.target)?;
        let symbol = self.nodes[selected]
            .symbol
            .and_then(|id| self.index.symbol(id))
            .context("select a function")?;
        let source = &self.sources[&symbol.file];
        let functions = startup.functions.clone();
        let function_aliases = startup.function_aliases.clone();
        let captured_defaults = startup.captured_defaults.clone();
        let contexts = startup.contexts.clone();
        let ambiguous = startup.ambiguous.clone();
        let inert_bindings = startup.inert_bindings.clone();
        let initialization = startup.initialization.clone();
        let entry = if let Some(modules) = &modules {
            modules.function_name(&symbol.file, &symbol.name)
        } else {
            symbol.name.clone()
        };
        let function = *functions
            .get(&entry)
            .context("select a top-level scalar function")?;
        ensure!(
            Span::from(function) == symbol.full_span,
            "selected function does not match top-level definition."
        );
        let rules = if let Some(path) = &options.rules {
            ensure!(
                std::fs::metadata(path)?.len() <= 65536,
                "rule file exceeds 64 KiB"
            );
            let rules: Rules = serde_json::from_slice(&crate::vfs::read(path)?)?;
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
                    !options.imports || !name.contains('.'),
                    "imported flow requires unqualified external rule names."
                );
                ensure!(
                    names.insert(name)
                        && !functions
                            .keys()
                            .any(|key| key.rsplit("::").next() == Some(name.as_str()))
                        && !inert_bindings
                            .iter()
                            .any(|key| key.rsplit("::").next() == Some(name.as_str()))
                        && !modules.as_ref().is_some_and(|m| m.has_imported_name(name)
                            || m.standard_enabled("builtins")
                                && standard_library::arity(name).is_some()),
                    "rule overlaps another rule or local definition."
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
        let configuration: BTreeMap<_, _> = self
            .manifests
            .snapshots
            .iter()
            .map(|(path, contents)| {
                (
                    path.strip_prefix(&self.root).unwrap_or(path),
                    json!(contents),
                )
            })
            .chain(self.lockfiles.snapshots.iter().map(|(path, contents)| {
                (
                    path.strip_prefix(&self.root).unwrap_or(path),
                    json!(contents),
                )
            }))
            .collect();
        let mut inputs = json!({
            "analyzer":super::flow_cache::analyzer_identity()?,
            "source":{"path":symbol.file.strip_prefix(&self.root)?,"digest":hash(source)?},
            "configuration":hash(configuration)?,
            "selection":{"name":symbol.name,"span":symbol.full_span},
            "rules":rules_digest,"context":options.context,
            "budget":{"steps":options.steps,"depth":options.depth,"bytes":options.bytes},
            "summary_mode":options.summaries,
            "completion_control":completion_contract(),
            "call_binding":if options.summaries {call_contract()} else {Value::Null},
            "assignment_control":if options.summaries {assignments::contract()} else {Value::Null},
            "expression_control":if options.summaries {expression_contract()} else {Value::Null},
            "dependency_scope":"entire defining file, negative same-file lookups, all indexed manifests/lockfiles, rules and analyzer; no imported execution."});
        if let Some(modules) = &modules {
            inputs["modules"] = modules.inputs.clone();
            inputs["dependency_scope"] = json!("static module closure, import candidates including missing paths, configuration, rules and analyzer.");
            inputs["rule_file"] = json!(options
                .rules
                .as_ref()
                .map(|path| path.strip_prefix(&self.root).unwrap_or(path)));
        }
        let input_digest = hash(&inputs)?;
        if options.inputs_only {
            let report = json!({"schema":"fr-dataflow-inputs-1","revision":self.revision,
                "handle_prefix":format!("frp1:{}:",&self.revision[..32]),"coverage":self.coverage(),
                "target":options.target,"input_digest":input_digest,"inputs":inputs});
            ensure!(
                serde_json::to_vec(&report)?.len() + 512 <= options.bytes,
                "response metadata exceeds byte budget"
            );
            return Ok(report);
        }
        if let Some(path) = &options.reuse {
            if let Some(report) = self.reuse_flow(
                path,
                options.reuse_digest.as_deref().unwrap(),
                &inputs,
                &symbol.file,
                &options.target,
            )? {
                ensure!(
                    serde_json::to_vec(&report)?.len() + 512 <= options.bytes,
                    "retained response exceeds byte budget"
                );
                return Ok(report);
            }
        }
        let mut analyzer = Analyzer {
            project: self,
            file: &symbol.file,
            source,
            functions,
            function_aliases,
            captured_defaults,
            ambiguous,
            inert_bindings,
            modules,
            contexts,
            rules,
            context: &options.context,
            remaining: allowance,
            depth: options.depth,
            active: Vec::new(),
            local_bindings: Vec::new(),
            cutoffs: BTreeSet::new(),
            effect_limits: BTreeMap::new(),
            events: Vec::new(),
            witnesses: BTreeMap::new(),
            graphs: BTreeMap::new(),
            summaries: Vec::new(),
            exceptional_returns: Flow::new(),
            current_block: None,
            origins: BTreeMap::new(),
            solver: summaries::Solver::new(options.summaries),
            normal: true,
            normal_return: false,
            return_text: None,
            may_raise: false,
            sink_flows: BTreeMap::new(),
        };
        if let Some(modules) = &modules {
            analyzer.cutoffs.extend(modules.cutoffs.clone());
            for (file, bindings) in &modules.bindings {
                for (alias, binding) in bindings {
                    if binding.member.is_some()
                        && !binding.future
                        && !modules.is_module(file, alias)
                        && !modules.is_value(file, alias)
                        && !analyzer
                            .functions
                            .contains_key(&modules.resolve(file, alias))
                    {
                        analyzer.cutoff(format!("missing-imported-member:{alias}"));
                    }
                }
            }
        }
        if !initialization.sites.is_empty() || initialization.omitted > 0 {
            analyzer.cutoff("module-effects-unchecked");
        }
        if self
            .manifests
            .snapshots
            .values()
            .any(|value| matches!(value, super::manifests::Snapshot::Skipped(_)))
            || self
                .lockfiles
                .snapshots
                .values()
                .any(|value| matches!(value, super::lockfiles::Snapshot::Skipped(_)))
        {
            analyzer.cutoff("configuration-snapshot-incomplete");
        }
        if analyzer.ambiguous.contains(&entry) {
            analyzer.cutoff("ambiguous-entry");
        }
        let parameters = analyzer.parameter_nodes(function).unwrap_or_default();
        let arguments = parameters
            .into_iter()
            .map(|p| {
                let occurrence = analyzer.occurrence(p, "entry-parameter");
                let origin = format!("parameter:{}", analyzer.text(p));
                analyzer.origins.insert(origin.clone(), occurrence.clone());
                BTreeMap::from([(
                    origin.clone(),
                    Trace {
                        origin,
                        occurrences: vec![occurrence],
                    },
                )])
            })
            .collect();
        let returns = if options.summaries {
            analyzer.solve_summaries(&entry, arguments)
        } else {
            analyzer.invoke(&entry, function, arguments)
        };
        let mut report = json!({"schema": "fr-dataflow-1", "revision": self.revision, "handle_prefix": format!("frp1:{}:", &self.revision[..32]), "coverage": self.coverage(), "target": options.target,
            "semantics": if options.summaries {"python-scalar-summaries"} else {"python-scalar-fixed-point"}, "claim": "possible-value-propagation",
            "scope": if options.imports {"selected function and static workspace-local module/package closure."} else {"selected function and direct helpers in the same file."},
            "complete": analyzer.cutoffs.is_empty(), "cutoffs": analyzer.cutoffs,
            "effect_limits":analyzer.effect_limits.into_values().collect::<Vec<_>>(),
            "runtime_effects":effect_contract(),
            "assumptions": ["scalar values and standard built-ins; no shared mutable aliases, monkey patching or implicit flows.", "branch feasibility unchecked",
                "external rules are caller-supplied contracts.", "explicit raises leave through modeled finally cleanup; implicit exceptions, handlers and resource effects are outside this model.", "finite origin sets; joins lose branch correlation; traces are derivations, not executable paths."],
            "rules_digest": rules_digest, "context": options.context,
            "input_digest":input_digest,"inputs":inputs,
            "execution":{"kind":"analyzed","analysis_steps":allowance-analyzer.remaining,
                "reason":if options.reuse.is_some() {"retained-result-not-reusable"} else {"no-retained-result"}},
            "events": analyzer.events, "returns": returns, "witnesses": analyzer.witnesses.into_values().collect::<Vec<_>>(),
            "origins":analyzer.origins,"control_flow":analyzer.graphs, "summaries":analyzer.summaries, "exceptional_returns":analyzer.exceptional_returns,
            "function_summaries":analyzer.solver.report(),
            "completion_control":completion_contract(),
            "call_binding":if options.summaries {call_contract()} else {Value::Null},
            "assignment_control":if options.summaries {assignments::contract()} else {Value::Null},
            "expression_control":if options.summaries {expression_contract()} else {Value::Null},
            "initialization":initialization,
            "completion":if options.summaries {json!({"normal_return":analyzer.normal_return,"may_raise":analyzer.may_raise})} else {Value::Null},
            "budget": {"steps": options.steps, "used": allowance - analyzer.remaining, "call_depth": options.depth, "response_bytes": options.bytes}});
        if !options.standard_library.is_empty() {
            report["assumptions"].as_array_mut().unwrap().push(json!("Selected standard-library models assume standard CPython imports and operations on scalars and immutable dates; standard-library implementation source is outside this check."));
        }
        let mut omitted = serde_json::Map::new();
        for field in [
            "initialization",
            "control_flow",
            "events",
            "summaries",
            "function_summaries",
            "witnesses",
            "exceptional_returns",
            "returns",
            "origins",
            "effect_limits",
            "call_binding",
            "assignment_control",
            "expression_control",
            "completion_control",
        ] {
            if serde_json::to_vec(&report)?.len() + 512 <= options.bytes {
                break;
            }
            let count = if field == "initialization" {
                report[field]["sites"].as_array().map_or(0, Vec::len)
            } else {
                report[field]
                    .as_array()
                    .map(Vec::len)
                    .or_else(|| report[field].as_object().map(serde_json::Map::len))
                    .unwrap_or(0)
            };
            if count == 0 {
                continue;
            }
            omitted.insert(field.into(), json!(count));
            report[field] = if field == "initialization" {
                json!({"sites":[],"omitted":report[field]["omitted"].as_u64().unwrap() + count as u64,
                    "by_file":report[field]["by_file"]})
            } else if matches!(
                field,
                "returns"
                    | "exceptional_returns"
                    | "control_flow"
                    | "origins"
                    | "function_summaries"
            ) {
                json!({})
            } else if matches!(
                field,
                "call_binding" | "assignment_control" | "expression_control" | "completion_control"
            ) {
                Value::Null
            } else {
                json!([])
            };
            report["complete"] = json!(false);
            report["omitted"] = json!(omitted);
            if !report["cutoffs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "response-budget")
            {
                report["cutoffs"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("response-budget"));
            }
        }
        ensure!(
            serde_json::to_vec(&report)?.len() + 512 <= options.bytes,
            "response metadata exceeds byte budget"
        );
        Ok(report)
    }
}

#[cfg(all(test, not(feature = "wasm")))]
mod lattice_tests {
    use super::*;
    use std::process::Command;

    fn environment(mask: usize, bound: bool) -> Environment {
        let flow = (0..4)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| {
                let origin = bit.to_string();
                (
                    origin.clone(),
                    Trace {
                        origin,
                        occurrences: vec![],
                    },
                )
            })
            .collect();
        Environment {
            values: BTreeMap::from([("value".into(), flow)]),
            plain_strings: BTreeSet::new(),
            pending_returns: BTreeMap::new(),
            pending_raises: BTreeMap::new(),
            defined: if bound {
                BTreeSet::from(["value".into()])
            } else {
                BTreeSet::new()
            },
        }
    }

    #[test]
    fn flow_join_and_definite_binding_match_lean_exhaustively() {
        let build = Command::new("lake")
            .args(["build", "fr-investigation-kernel", "--wfail"])
            .current_dir("kernels")
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let output = Command::new("lake")
            .args(["exe", "fr-investigation-kernel", "flow-join"])
            .current_dir("kernels")
            .output()
            .unwrap();
        assert!(output.status.success());
        let mut expected = Vec::new();
        for left in 0..16 {
            for right in 0..16 {
                for bound_left in [false, true] {
                    for bound_right in [false, true] {
                        let mut state = environment(left, bound_left);
                        let changed =
                            join_environment(&mut state, &environment(right, bound_right));
                        let mask: usize = state.values["value"]
                            .keys()
                            .map(|key| 1 << key.parse::<usize>().unwrap())
                            .sum();
                        expected.push(format!(
                            "{mask} {} {changed}",
                            state.defined.contains("value")
                        ));
                    }
                }
            }
        }
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            expected
        );
    }
}
