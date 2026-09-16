# Completion audit

`fr audit` gives an agent a small support and trust summary before it chooses a workflow. The
summary derives counts from live predicates and catalogs. It also gives exact commands for seven
bounded detail sections.

```sh
fr audit
fr --json audit workflows
fr --json audit boundaries
```

Every report carries `fr-completion-audit-1` and a Merkle object root. The summary reports parser,
capability, workflow, recipe and application counts. Detail sections expose the underlying rows,
schemas, commands, source policy, proof classes and refusal reasons.

The report keeps four questions separate:

- A support predicate says whether a route admits a language, adapter or feature class.
- An executable acceptance target says where the repository tests that route.
- A retained evaluation records what one fixed run observed.
- A proof names only its model, assumptions and checked correspondence boundary.

An unrun test remains an obligation. A supported route can refuse an exact input. A model theorem
does not establish runtime behavior or general source equivalence.

## Deterministic workflow sweep

`tools/completion-workflows.py` exercises understanding, tracing, direct change, recipe, semantic
edit, framework migration and proof guidance on an isolated fixture. Each route executes at least
one returned read or preview action. The tool verifies that these previews leave all source bytes
unchanged.

The retained [workflow report](../tests/agent-eval/completion-workflows.json) records fourteen real
manual discovery calls. The guide replaces them with seven structured goal calls and leaves zero
exploratory help, vocabulary, schema or audit calls after guidance. It records complete request and
response byte counts for each discovery and guide call.

The deterministic report does not run a model. It omits hidden reasoning, tokenizer counts and
billed quota. Fresh Codex evidence records those fields separately when the CLI exposes them.

Regenerate and verify the report with:

```sh
python3 tools/completion-workflows.py --fr target/debug/fr \
  --output tests/agent-eval/completion-workflows.json
python3 tools/completion-workflows.py \
  --audit tests/agent-eval/completion-workflows.json
```

The report binds the evaluator and implementing source files by SHA-256. Its auditor refuses stale
bindings, missing workflow families, source changes, absent action execution or overstated context
claims.
