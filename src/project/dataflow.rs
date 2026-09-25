use super::{
    control_flow::{Graph, Operation},
    hash,
    occurrence::Occurrence,
    Project,
};
use crate::{lang::Language, parse::Parsers, span::Span};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use tree_sitter::Node;

#[path = "flow_modules.rs"]
mod modules;
#[path = "flow_summaries.rs"]
mod summaries;

#[derive(Args)]
pub struct Options {
    target: String,
    #[arg(
        long,
        help = "Solve symbolic function summaries, including same-file recursion."
    )]
    summaries: bool,
    #[arg(
        long,
        requires = "summaries",
        help = "Follow static root-local Python module imports."
    )]
    imports: bool,
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
            summaries: true,
            imports,
            rules,
            steps,
            depth,
            context: context.into(),
            bytes: 1_048_576,
            inputs_only: false,
            reuse: None,
            reuse_digest: None,
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
struct Environment {
    values: BTreeMap<String, Flow>,
    defined: BTreeSet<String>,
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
    changed
}

struct Analyzer<'a, 'p, 't> {
    project: &'a Project<'p>,
    file: &'a Path,
    source: &'a str,
    functions: BTreeMap<String, Node<'t>>,
    ambiguous: BTreeSet<String>,
    modules: Option<&'a modules::Modules>,
    contexts: BTreeMap<String, (&'a Path, &'a str)>,
    rules: Rules,
    context: &'a str,
    remaining: usize,
    depth: usize,
    active: Vec<String>,
    local_bindings: Vec<BTreeSet<String>>,
    cutoffs: BTreeSet<String>,
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
            | "unary_operator"
            | "boolean_operator"
            | "comparison_operator"
            | "parenthesized_expression" => {
                if self.solver.enabled
                    && matches!(node.kind(), "boolean_operator" | "comparison_operator")
                {
                    let mut pending = vec![node];
                    while let Some(operand) = pending.pop() {
                        if operand.kind() == "call" {
                            self.cutoff("short-circuit-call-control-unchecked");
                            break;
                        }
                        pending.extend(operand.named_children(&mut operand.walk()));
                    }
                }
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
        if function.kind() != "identifier"
            && !(self.modules.is_some()
                && function.kind() == "attribute"
                && function
                    .child_by_field_name("object")
                    .is_some_and(|n| n.kind() == "identifier")
                && function
                    .child_by_field_name("attribute")
                    .is_some_and(|n| n.kind() == "identifier"))
        {
            self.cutoff("dynamic-or-attribute-call");
            return Flow::new();
        }
        let name = if function.kind() == "attribute" {
            format!(
                "{}.{}",
                self.text(function.child_by_field_name("object").unwrap()),
                self.text(function.child_by_field_name("attribute").unwrap())
            )
        } else {
            self.text(function).to_owned()
        };
        let mut arguments = Vec::new();
        if let Some(args) = node.child_by_field_name("arguments") {
            for arg in args.named_children(&mut args.walk()) {
                arguments.push(self.expression(arg, env));
            }
        }
        let mut joined = Flow::new();
        if self.solver.enabled && !self.normal {
            return Flow::new();
        }
        for argument in &arguments {
            merge(&mut joined, argument.clone());
        }
        let base = name.split('.').next().unwrap();
        let resolved = self
            .modules
            .map_or_else(|| name.clone(), |modules| modules.resolve(self.file, &name));
        if env.values.contains_key(base)
            || self.ambiguous.contains(&resolved)
            || self
                .local_bindings
                .last()
                .is_some_and(|names| names.contains(base))
        {
            self.cutoff(format!("ambiguous-call:{name}"));
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
        let Some(function) = self.functions.get(&resolved).copied() else {
            self.cutoff(format!("unknown-external-call:{name}"));
            return joined;
        };
        if self.solver.enabled {
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
        if function.child_by_field_name("return_type").is_some() {
            self.cutoff("function-annotation-effects");
        }
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
            if matches!(
                node.kind(),
                "assignment" | "augmented_assignment" | "named_expression"
            ) {
                if let Some(left) = node
                    .child_by_field_name("left")
                    .or_else(|| node.child_by_field_name("name"))
                {
                    if left.kind() == "identifier" {
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
            self.normal = true;
            if self.solver.enabled && block.operation == Operation::Exit {
                self.normal_return = true;
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
                        if self.solver.enabled
                            && block.operation == Operation::Raise
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
                        if self.solver.enabled && !self.normal {
                            continue;
                        }
                        if block.operation == Operation::Return {
                            self.normal_return = true;
                            merge(&mut result, value);
                        } else {
                            self.may_raise = true;
                            if !self.solver.enabled && self.active.len() > 1 {
                                self.cutoff("helper-exception-control-unchecked");
                            }
                            merge(&mut self.exceptional_returns, value);
                        }
                    }
                    _ => (),
                }
            }
            if self.solver.enabled && !self.normal {
                continue;
            }
            for (successor, _) in &block.successors {
                let changed = match &mut incoming[*successor] {
                    Some(previous) => join_environment(previous, &state),
                    slot @ None => {
                        *slot = Some(state.clone());
                        true
                    }
                };
                if changed && queued.insert(*successor) {
                    queue.push_back(*successor);
                }
            }
        }
        self.active.pop();
        self.local_bindings.pop();
        self.current_block = parent_block;
        self.summaries.push(json!({"function":name,"inputs":inputs,"return_origins":result.keys().collect::<Vec<_>>(),
            "block_visits":visits,"converged":queue.is_empty(),"claim":if self.solver.enabled {"one symbolic summary evaluation; global convergence reported separately"} else {"context-specific evaluation; recursive summaries unsupported"}}));
        result
    }

    fn statement(&mut self, node: Node<'t>, state: &mut Environment) {
        match node.kind() {
            "expression_statement" => {
                for expression in node.named_children(&mut node.walk()) {
                    if expression.kind() == "assignment" {
                        let (Some(left), Some(right)) = (
                            expression.child_by_field_name("left"),
                            expression.child_by_field_name("right"),
                        ) else {
                            self.cutoff("unsupported-assignment");
                            continue;
                        };
                        let value = self.expression(right, state);
                        if left.kind() == "identifier" {
                            let name = self.text(left).to_owned();
                            state
                                .values
                                .insert(name.clone(), self.extend(value, left, "definition"));
                            state.defined.insert(name);
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
}

impl Project<'_> {
    pub(super) fn flow_dependency(&self, key: &str) -> Result<String> {
        let query: DependencyQuery = serde_json::from_str(key)?;
        let path = Path::new(&query.path);
        ensure!(
            path.components().count() == 1
                && path
                    .file_name()
                    .is_some_and(|name| name == path.as_os_str())
                && path.extension().is_some_and(|extension| extension == "py"),
            "flow dependency needs a root-local Python file."
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
            summaries: true,
            imports: true,
            rules,
            context: query.context,
            steps: query.steps,
            depth: query.depth,
            bytes: query.bytes,
            inputs_only: true,
            reuse: None,
            reuse_digest: None,
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
        let source = &self.sources[&symbol.file];
        let parsed = Parsers::new().parse(Language::Python, source)?;
        ensure!(!parsed.root().has_error(), "dataflow refuses syntax errors");
        let modules = options
            .imports
            .then(|| modules::Modules::load(self, &symbol.file))
            .transpose()?;
        let mut functions = BTreeMap::new();
        let mut contexts = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();
        let mut module_effects = false;
        let units: Vec<_> = if let Some(modules) = &modules {
            modules
                .parsed
                .iter()
                .map(|(file, parsed)| (file.as_path(), self.sources[file].as_str(), parsed))
                .collect()
        } else {
            vec![(symbol.file.as_path(), source.as_str(), &parsed)]
        };
        for (file, text, parsed) in units {
            for node in parsed.root().named_children(&mut parsed.root().walk()) {
                if node.kind() == "function_definition" {
                    if node.child_by_field_name("return_type").is_some()
                        || node
                            .child_by_field_name("parameters")
                            .is_some_and(|parameters| {
                                parameters
                                    .named_children(&mut parameters.walk())
                                    .any(|p| p.kind() != "identifier")
                            })
                    {
                        module_effects = true;
                    }
                    let Some(name_node) = node.child_by_field_name("name") else {
                        continue;
                    };
                    let short = &text[name_node.byte_range()];
                    let name = if options.imports {
                        format!("{}::{short}", file.file_name().unwrap().to_string_lossy())
                    } else {
                        short.to_owned()
                    };
                    if functions.insert(name.clone(), node).is_some() {
                        ambiguous.insert(name.clone());
                    }
                    contexts.insert(name, (file, text));
                } else if !(options.imports
                    && matches!(node.kind(), "import_statement" | "import_from_statement"))
                    && (!matches!(node.kind(), "comment" | "expression_statement")
                        || (node.kind() == "expression_statement"
                            && node.named_child(0).is_some_and(|n| n.kind() != "string")))
                {
                    module_effects = true;
                }
            }
        }
        let entry = if options.imports {
            format!(
                "{}::{}",
                symbol.file.file_name().unwrap().to_string_lossy(),
                symbol.name
            )
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
                        && !modules.as_ref().is_some_and(|m| m
                            .bindings
                            .values()
                            .any(|bindings| bindings.contains_key(name))),
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
            ambiguous,
            modules: modules.as_ref(),
            contexts,
            rules,
            context: &options.context,
            remaining: options.steps,
            depth: options.depth,
            active: Vec::new(),
            local_bindings: Vec::new(),
            cutoffs: BTreeSet::new(),
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
            may_raise: false,
            sink_flows: BTreeMap::new(),
        };
        if let Some(modules) = &modules {
            analyzer.cutoffs.extend(modules.cutoffs.clone());
            for (file, bindings) in &modules.bindings {
                for (alias, binding) in bindings {
                    if binding.member.is_some()
                        && !analyzer
                            .functions
                            .contains_key(&modules.resolve(file, alias))
                    {
                        analyzer.cutoff(format!("missing-imported-member:{alias}"));
                    }
                }
            }
        }
        if module_effects {
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
        let parameters = function
            .child_by_field_name("parameters")
            .context("missing parameters")?;
        let arguments = parameters
            .named_children(&mut parameters.walk())
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
            "semantics": if options.summaries {"python-scalar-summaries-1"} else {"python-scalar-fixed-point-2"}, "claim": "possible-value-propagation",
            "scope": if options.imports {"selected function and static root-local module closure."} else {"selected function and direct helpers in the same file."},
            "complete": analyzer.cutoffs.is_empty(), "cutoffs": analyzer.cutoffs,
            "assumptions": ["scalar values; no aliases, monkey patching or implicit flows.", "branch feasibility unchecked",
                "external rules are caller-supplied contracts.", "explicit raises terminate; implicit exceptions, handlers and resource effects are outside this model.", "finite origin sets; joins lose branch correlation; traces are derivations, not executable paths."],
            "rules_digest": rules_digest, "context": options.context,
            "input_digest":input_digest,"inputs":inputs,
            "execution":{"kind":"analyzed","analysis_steps":options.steps-analyzer.remaining,
                "reason":if options.reuse.is_some() {"retained-result-not-reusable"} else {"no-retained-result"}},
            "events": analyzer.events, "returns": returns, "witnesses": analyzer.witnesses.into_values().collect::<Vec<_>>(),
            "origins":analyzer.origins,"control_flow":analyzer.graphs, "summaries":analyzer.summaries, "exceptional_returns":analyzer.exceptional_returns,
            "function_summaries":analyzer.solver.report(),
            "completion":if options.summaries {json!({"normal_return":analyzer.normal_return,"may_raise":analyzer.may_raise})} else {Value::Null},
            "budget": {"steps": options.steps, "used": options.steps - analyzer.remaining, "call_depth": options.depth, "response_bytes": options.bytes}});
        let mut omitted = serde_json::Map::new();
        for field in [
            "control_flow",
            "events",
            "summaries",
            "function_summaries",
            "witnesses",
            "exceptional_returns",
            "returns",
            "origins",
        ] {
            if serde_json::to_vec(&report)?.len() + 512 <= options.bytes {
                break;
            }
            let count = report[field]
                .as_array()
                .map(Vec::len)
                .or_else(|| report[field].as_object().map(serde_json::Map::len))
                .unwrap_or(0);
            omitted.insert(field.into(), json!(count));
            report[field] = if matches!(
                field,
                "returns"
                    | "exceptional_returns"
                    | "control_flow"
                    | "origins"
                    | "function_summaries"
            ) {
                json!({})
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
