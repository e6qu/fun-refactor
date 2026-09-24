use super::{dataflow, flow_fact_origins::Linker, hash, occurrence::Occurrence, page, Project};
use anyhow::{ensure, Context, Result};
use clap::Args;
use serde_json::{json, Value};
use std::path::PathBuf;

const RULE: &str = "python-scalar-facts-1";

pub fn fact_catalogue_complete(analysis: bool, from_start: bool, no_remaining: bool) -> bool {
    analysis && from_start && no_remaining
}

#[derive(Args)]
pub struct Options {
    target: String,
    #[arg(long)]
    rules: Option<PathBuf>,
    #[arg(long, default_value = "generic")]
    context: String,
    #[arg(long, default_value_t = 1024)]
    steps: usize,
    #[arg(long, default_value_t = 8)]
    depth: usize,
    #[arg(long, default_value_t = 16)]
    limit: usize,
    #[arg(long, conflicts_with = "fact")]
    cursor: Option<String>,
    #[arg(long)]
    fact: Option<String>,
    #[arg(long, default_value_t = 8)]
    evidence_limit: usize,
    #[arg(long, requires = "fact")]
    evidence_cursor: Option<String>,
    #[arg(long, default_value_t = 65536)]
    bytes: usize,
}

struct Fact {
    row: Value,
    evidence: Vec<Occurrence>,
}

fn add(
    facts: &mut Vec<Fact>,
    report: &Value,
    basis: &str,
    kind: &str,
    function: &str,
    conclusion: Value,
    evidence: Value,
) -> Result<()> {
    let evidence: Vec<Occurrence> = serde_json::from_value(evidence)?;
    let core = json!({"kind":kind,"function":function,"conclusion":conclusion,
        "rule":format!("{RULE}:{kind}"),"input_digest":report["input_digest"],
        "confidence":if kind == "boundary" {"observed-cutoff"} else {"model-derived"},
        "scope":if kind.starts_with("summary-") {"symbolic-function"} else {"selected-entry"},
        "analysis_complete":report["complete"],"assumptions_ref":"/analysis/assumptions",
        "omissions_ref":"/analysis/omitted","evidence_count":evidence.len(),"evidence_digest":hash(serde_json::to_value(&evidence)?)?});
    let id = format!("frff1:{}", hash((basis, &core))?);
    facts.push(Fact {
        row: json!({"id":id,"basis":basis,"core":core}),
        evidence,
    });
    Ok(())
}

fn facts(report: &Value, basis: &str) -> Result<Vec<Fact>> {
    let mut rows = Vec::new();
    for kind in ["returns", "exceptional_returns"] {
        if let Some(flows) = report[kind].as_object() {
            for (origin, trace) in flows {
                add(
                    &mut rows,
                    report,
                    basis,
                    if kind == "returns" { "return" } else { "raise" },
                    "entry",
                    json!({"origin":origin}),
                    trace["occurrences"].clone(),
                )?;
            }
        }
    }
    if let Some(witnesses) = report["witnesses"].as_array() {
        for witness in witnesses {
            add(
                &mut rows,
                report,
                basis,
                "witness",
                "entry",
                json!({"origin":witness["trace"]["origin"],
                "sink":witness["sink"],"site":witness["site"]["id"],"context":witness["context"]}),
                witness["trace"]["occurrences"].clone(),
            )?;
        }
    }
    if report["completion"].is_object() {
        add(
            &mut rows,
            report,
            basis,
            "completion",
            "entry",
            report["completion"].clone(),
            json!([]),
        )?;
    }
    if let Some(functions) = report["function_summaries"]["functions"].as_object() {
        for (name, summary) in functions {
            for (field, kind) in [
                ("returns", "summary-return"),
                ("exceptional_returns", "summary-raise"),
            ] {
                for (origin, trace) in summary[field].as_object().into_iter().flatten() {
                    add(
                        &mut rows,
                        report,
                        basis,
                        kind,
                        name,
                        json!({"origin":origin}),
                        trace["occurrences"].clone(),
                    )?;
                }
            }
            for effect in summary["sinks"]
                .as_object()
                .into_iter()
                .flat_map(|object| object.values())
            {
                for (origin, trace) in effect["flow"].as_object().into_iter().flatten() {
                    add(
                        &mut rows,
                        report,
                        basis,
                        "summary-sink",
                        name,
                        json!({"origin":origin,"sink":effect["sink"],
                        "site":effect["site"]["id"],"context":report["context"]}),
                        trace["occurrences"].clone(),
                    )?;
                }
            }
            add(
                &mut rows,
                report,
                basis,
                "summary-completion",
                name,
                json!({"normal_return":summary["normal_return"],"may_raise":summary["may_raise"],
                    "evaluations":summary["evaluations"],"converged":report["function_summaries"]["converged"]}),
                json!([]),
            )?;
        }
    }
    for reason in report["cutoffs"].as_array().into_iter().flatten() {
        add(
            &mut rows,
            report,
            basis,
            "boundary",
            "analysis",
            json!({"reason":reason}),
            json!([]),
        )?;
    }
    Ok(rows)
}

impl Options {
    fn arguments(&self) -> Vec<String> {
        let mut args = vec![
            "project".into(),
            "flow-facts".into(),
            self.target.clone(),
            "--steps".into(),
            self.steps.to_string(),
            "--depth".into(),
            self.depth.to_string(),
            "--context".into(),
            self.context.clone(),
            "--limit".into(),
            self.limit.to_string(),
            "--evidence-limit".into(),
            self.evidence_limit.to_string(),
            "--bytes".into(),
            self.bytes.to_string(),
        ];
        if let Some(rules) = &self.rules {
            args.extend(["--rules".into(), rules.to_string_lossy().into_owned()]);
        }
        args
    }

    fn action(&self, fact: Option<&str>, cursor: Option<&str>) -> Value {
        let mut arguments = self.arguments();
        if let Some(id) = fact {
            arguments.extend(["--fact".into(), id.into()]);
        }
        if let Some(cursor) = cursor {
            arguments.extend([
                if fact.is_some() {
                    "--evidence-cursor"
                } else {
                    "--cursor"
                }
                .into(),
                cursor.into(),
            ]);
        }
        json!({"arguments":arguments})
    }
}

impl Project<'_> {
    pub(super) fn flow_facts(&self, options: &Options) -> Result<Value> {
        ensure!(
            (1..=64).contains(&options.limit) && (1..=64).contains(&options.evidence_limit),
            "fact and evidence limits must be between 1 and 64."
        );
        ensure!(
            (4096..=1_048_576).contains(&options.bytes),
            "fact response budget must be between 4096 and 1048576."
        );
        let analysis = self.dataflow(&dataflow::Options::for_facts(
            &options.target,
            options.rules.clone(),
            &options.context,
            options.steps,
            options.depth,
        ))?;
        let analyzer = hash((
            RULE,
            include_str!("flow_facts.rs"),
            include_str!("flow_fact_origins.rs"),
        ))?;
        let basis = format!(
            "frfb1:{}",
            hash((&self.revision, &analysis["input_digest"], &analyzer))?
        );
        let rows = facts(&analysis, &basis)?;
        let mut result = json!({"schema":"fr-flow-facts-1","revision":self.revision,"basis":basis,
            "handle_prefix":analysis["handle_prefix"],"coverage":analysis["coverage"],
            "target":options.target,"analyzer":analyzer,"rule_version":RULE,
            "analysis":{"input_digest":analysis["input_digest"],"inputs":analysis["inputs"],"semantics":analysis["semantics"],
                "complete":analysis["complete"],"cutoffs":analysis["cutoffs"],"omitted":analysis.get("omitted").cloned().unwrap_or(json!({})),
                "assumptions":analysis["assumptions"],"rules_digest":analysis["rules_digest"],"context":analysis["context"],
                "execution":analysis["execution"]},
            "claim":"model-derived value propagation; neither executable paths nor source implementation proofs.",
            "mutation_authority":false,"items":[],"evidence":[],"fact":Value::Null,
            "disclosure":{"kind":if options.fact.is_some() {"fact-evidence"} else {"fact-catalogue"}},
            "budget":{"response_bytes":options.bytes,"semantic_queries":8},
            "continuation":Value::Null});
        let key = format!("frff-page:{}", hash((&basis, &options.fact))?);
        let mut linker = Linker::new(self);
        let (total, limit, cursor) = if let Some(id) = &options.fact {
            let fact = rows
                .iter()
                .find(|row| row.row["id"] == *id)
                .context("unknown or stale flow fact; restart the fact catalogue.")?;
            result["fact"] = fact.row.clone();
            result["fact"]["follow"] = options.action(Some(id), None);
            (
                fact.evidence.len(),
                options.evidence_limit,
                options.evidence_cursor.as_deref(),
            )
        } else {
            (rows.len(), options.limit, options.cursor.as_deref())
        };
        let (start, end, _) = page(total, limit, cursor, &key)?;
        let field = if options.fact.is_some() {
            "evidence"
        } else {
            "items"
        };
        for index in start..end {
            let item = if let Some(id) = &options.fact {
                let fact = rows.iter().find(|row| row.row["id"] == *id).unwrap();
                let point = &fact.evidence[index];
                let mut item = linker.explain(point)?;
                item["index"] = json!(index);
                item["rule"] = occurrence_rule(&point.role, &analysis["rules_digest"]);
                item
            } else {
                let mut row = rows[index].row.clone();
                row["follow"] = options.action(row["id"].as_str(), None);
                row
            };
            result[field].as_array_mut().unwrap().push(item);
        }
        loop {
            let returned = result[field].as_array().unwrap().len();
            let end = start + returned;
            let next = (end < total).then(|| format!("{key}:{end}"));
            result["page"] = json!({"before":start,"returned":returned,"remaining":total-end,"total":total,"next":next});
            result["continuation"] = next
                .as_deref()
                .map(|cursor| options.action(options.fact.as_deref(), Some(cursor)))
                .unwrap_or(Value::Null);
            result["disclosure"]["complete"] = json!(start == 0 && end == total);
            result["complete"] = json!(fact_catalogue_complete(
                analysis["complete"] == true,
                start == 0,
                end == total
            ));
            result["semantic_queries"] = json!(linker.queries());
            if serde_json::to_vec(&result)?.len() + 512 <= options.bytes {
                break;
            }
            ensure!(
                returned > 0,
                "fact metadata exceeds response budget; increase --bytes."
            );
            result[field].as_array_mut().unwrap().pop();
            ensure!(
                returned > 1 || total == 0,
                "one fact or explanation exceeds response budget; increase --bytes."
            );
        }
        Ok(result)
    }
}

fn occurrence_rule(role: &str, external_digest: &Value) -> Value {
    let contract = matches!(
        role,
        "source" | "sink" | "propagation-rule" | "sanitizer-context-mismatch"
    );
    let claim = match role {
        "source" => "The caller supplied this source contract.",
        "sink" => "The caller supplied this sink contract.",
        "parameter" | "entry-parameter" | "summary-parameter" => {
            "This occurrence binds a positional parameter."
        }
        "definition" => "This assignment replaces prior scalar origins.",
        "use" => "This use reads the current abstract scalar value.",
        "return" => "This explicit return carries the recorded origins.",
        "raise" | "summary-raise" => {
            "This explicit exceptional effect carries the recorded origins."
        }
        "call-result" | "summary-call" => {
            "This call substitutes arguments into a symbolic function summary."
        }
        "expression" => "This expression combines explicit operand origins.",
        "sanitizer-context-mismatch" => {
            "The sanitizer contract does not apply in the selected context."
        }
        "propagation-rule" => "The caller supplied this propagation contract.",
        _ => "This is a recorded syntax occurrence in a model derivation.",
    };
    json!({"id":format!("{RULE}:occurrence:{role}"),"claim":claim,
        "external_rules_digest":if contract {external_digest.clone()} else {Value::Null},
        "relation":"derivation membership; no feasible-path or per-edge proof"})
}
