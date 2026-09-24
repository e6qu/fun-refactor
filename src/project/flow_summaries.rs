use super::*;

const FUNCTION_LIMIT: usize = 64;

#[derive(Clone, Serialize)]
pub(super) struct SinkFlow {
    sink: String,
    site: Occurrence,
    flow: Flow,
}

#[derive(Clone, Default, Serialize)]
struct Summary {
    parameters: Vec<String>,
    returns: Flow,
    exceptional_returns: Flow,
    sinks: BTreeMap<String, SinkFlow>,
    normal_return: bool,
    may_raise: bool,
    callees: BTreeSet<String>,
    evaluations: usize,
}

pub(super) struct Solver {
    pub enabled: bool,
    table: BTreeMap<String, Summary>,
    depths: BTreeMap<String, usize>,
    callees: BTreeSet<String>,
    rounds: usize,
    converged: bool,
}

impl Solver {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            table: BTreeMap::new(),
            depths: BTreeMap::new(),
            callees: BTreeSet::new(),
            rounds: 0,
            converged: false,
        }
    }

    pub fn report(&self) -> Value {
        json!({"schema":"fr-function-summaries-1", "enabled":self.enabled,
            "converged":self.converged,"rounds":self.rounds,"functions":self.table,
            "function_limit":FUNCTION_LIMIT,
            "context":"symbolic positional parameters; substitute independently at each call.",
            "recursion":"monotone least fixed point over finite origin and effect sets.",
            "effects":"explicit scalar returns, sink contracts and explicit raises; no heap or imported execution.",
            "claim":"may-value derivations in the declared model; neither feasible paths nor runtime termination.",
            "mutation_authority":false})
    }
}

fn add(into: &mut Flow, from: Flow) -> bool {
    let before = into.len();
    merge(into, from);
    into.len() != before
}

fn add_sinks(into: &mut BTreeMap<String, SinkFlow>, from: BTreeMap<String, SinkFlow>) -> bool {
    let mut changed = false;
    for (key, effect) in from {
        if let Some(old) = into.get_mut(&key) {
            changed |= add(&mut old.flow, effect.flow);
        } else {
            into.insert(key, effect);
            changed = true;
        }
    }
    changed
}

fn append(trace: &mut Trace, occurrences: &[Occurrence]) {
    for occurrence in occurrences {
        if !trace.occurrences.iter().any(|old| old.id == occurrence.id) {
            trace.occurrences.push(occurrence.clone());
        }
    }
}

fn substitute(template: &Flow, parameters: &[String], arguments: &[Flow]) -> Flow {
    let mut result = Flow::new();
    for (origin, trace) in template {
        if let Some(index) = parameters.iter().position(|parameter| parameter == origin) {
            for (input, argument) in &arguments[index] {
                let mut value = argument.clone();
                append(&mut value, &trace.occurrences);
                result.entry(input.clone()).or_insert(value);
            }
        } else {
            result
                .entry(origin.clone())
                .or_insert_with(|| trace.clone());
        }
    }
    result
}

impl<'a, 'p, 't> Analyzer<'a, 'p, 't> {
    pub(super) fn record_sink(&mut self, name: &str, site: Occurrence, flow: Flow) {
        let effect = self
            .sink_flows
            .entry(site.id.clone())
            .or_insert_with(|| SinkFlow {
                sink: name.into(),
                site,
                flow: Flow::new(),
            });
        merge(&mut effect.flow, flow);
    }

    pub(super) fn summary_call(
        &mut self,
        name: &str,
        function: Node<'t>,
        arguments: Vec<Flow>,
        call: Node<'t>,
    ) -> Flow {
        let valid = function
            .child_by_field_name("parameters")
            .is_some_and(|parameters| {
                let items: Vec<_> = parameters.named_children(&mut parameters.walk()).collect();
                items.len() == arguments.len() && items.iter().all(|p| p.kind() == "identifier")
            });
        if !valid {
            self.cutoff(format!("unsupported-parameter-contract:{name}"));
            return Flow::new();
        }
        self.solver.callees.insert(name.into());
        if !self.solver.table.contains_key(name) {
            let parent = self.active.last().expect("summary evaluation has a caller");
            let depth = self.solver.depths[parent] + 1;
            if depth >= self.depth {
                self.cutoff("call-depth-budget");
                return Flow::new();
            }
            if self.solver.table.len() == FUNCTION_LIMIT {
                self.cutoff("summary-function-budget");
                return Flow::new();
            }
            self.solver.depths.insert(name.into(), depth);
            self.solver.table.insert(name.into(), Summary::default());
        }
        let summary = self.solver.table[name].clone();
        for effect in summary.sinks.values() {
            let flow = substitute(&effect.flow, &summary.parameters, &arguments);
            let flow = self.extend(flow, call, "summary-call");
            self.record_sink(&effect.sink, effect.site.clone(), flow);
        }
        let exceptions = substitute(
            &summary.exceptional_returns,
            &summary.parameters,
            &arguments,
        );
        let exceptions = self.extend(exceptions, call, "summary-raise");
        merge(&mut self.exceptional_returns, exceptions);
        self.may_raise |= summary.may_raise;
        self.normal = summary.normal_return;
        let result = substitute(&summary.returns, &summary.parameters, &arguments);
        self.extend(result, call, "call-result")
    }

    pub(super) fn solve_summaries(&mut self, entry: &str, arguments: Vec<Flow>) -> Flow {
        self.solver.table.insert(entry.into(), Summary::default());
        self.solver.depths.insert(entry.into(), 0);
        loop {
            self.solver.rounds += 1;
            let names: Vec<_> = self.solver.table.keys().cloned().collect();
            let mut changed = false;
            for name in &names {
                if self.remaining == 0 {
                    self.cutoff("step-budget");
                    break;
                }
                let function = self.functions[name];
                let parameters: Vec<_> = function
                    .child_by_field_name("parameters")
                    .unwrap()
                    .named_children(&mut function.walk())
                    .enumerate()
                    .map(|(index, node)| {
                        let key = format!("argument:{name}:{index}");
                        let occurrence = self.occurrence(node, "summary-parameter");
                        (
                            key.clone(),
                            BTreeMap::from([(
                                key.clone(),
                                Trace {
                                    origin: key,
                                    occurrences: vec![occurrence],
                                },
                            )]),
                        )
                    })
                    .collect();
                self.normal_return = false;
                self.may_raise = false;
                self.exceptional_returns.clear();
                self.sink_flows.clear();
                self.solver.callees.clear();
                let returns = self.invoke(
                    name,
                    function,
                    parameters.iter().map(|(_, flow)| flow.clone()).collect(),
                );
                let summary = self.solver.table.get_mut(name).unwrap();
                summary.parameters = parameters.into_iter().map(|(key, _)| key).collect();
                summary.evaluations += 1;
                summary.callees.extend(self.solver.callees.iter().cloned());
                changed |= add(&mut summary.returns, returns);
                changed |= add(
                    &mut summary.exceptional_returns,
                    std::mem::take(&mut self.exceptional_returns),
                );
                changed |= add_sinks(&mut summary.sinks, std::mem::take(&mut self.sink_flows));
                changed |= (self.normal_return && !summary.normal_return)
                    || (self.may_raise && !summary.may_raise);
                summary.normal_return |= self.normal_return;
                summary.may_raise |= self.may_raise;
            }
            if self.remaining == 0 {
                self.cutoff("step-budget");
                break;
            }
            if !changed && names.len() == self.solver.table.len() {
                self.solver.converged = true;
                break;
            }
        }
        let summary = self.solver.table[entry].clone();
        self.normal_return = summary.normal_return;
        self.may_raise = summary.may_raise;
        self.exceptional_returns = substitute(
            &summary.exceptional_returns,
            &summary.parameters,
            &arguments,
        );
        for effect in summary.sinks.values() {
            for trace in substitute(&effect.flow, &summary.parameters, &arguments).values() {
                let key = format!("{:020}:{}", effect.site.location.span.start, trace.origin);
                self.witnesses.insert(
                    key,
                    json!({"sink":effect.sink,"context":self.context,
                    "rules":self.rules.version,"trace":trace,"site":effect.site,
                    "claim":"possible-value-propagation; path feasibility unchecked."}),
                );
            }
        }
        substitute(&summary.returns, &summary.parameters, &arguments)
    }
}

#[cfg(all(test, not(feature = "wasm")))]
mod tests {
    use super::*;
    use std::process::Command;

    fn flow(mask: usize) -> Flow {
        (0..4)
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
            .collect()
    }

    #[test]
    fn symbolic_substitution_matches_all_1024_lean_cases() {
        let built = Command::new("lake")
            .args(["build", "fr-investigation-kernel", "--wfail"])
            .current_dir("kernels")
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}{}",
            String::from_utf8_lossy(&built.stdout),
            String::from_utf8_lossy(&built.stderr)
        );
        let result = Command::new("lake")
            .args(["exe", "fr-investigation-kernel", "flow-summary"])
            .current_dir("kernels")
            .output()
            .unwrap();
        assert!(result.status.success());
        let mut expected = Vec::new();
        let parameters: Vec<String> = vec!["first".into(), "second".into()];
        for first in [false, true] {
            for second in [false, true] {
                let template: Flow = parameters
                    .iter()
                    .zip([first, second])
                    .filter(|(_, enabled)| *enabled)
                    .map(|(key, _)| {
                        (
                            key.clone(),
                            Trace {
                                origin: key.clone(),
                                occurrences: vec![],
                            },
                        )
                    })
                    .collect();
                for left in 0..16 {
                    for right in 0..16 {
                        let output = substitute(&template, &parameters, &[flow(left), flow(right)]);
                        let mask = output
                            .keys()
                            .map(|key| 1 << key.parse::<usize>().unwrap())
                            .sum::<usize>();
                        expected.push(mask.to_string());
                    }
                }
            }
        }
        assert_eq!(
            String::from_utf8(result.stdout)
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            expected
        );
    }
}
