# Completion audit

`fr audit` gives an agent a compact support and trust summary before it chooses a workflow. The
summary derives its counts from live predicates and catalogs.

```sh
fr audit
fr --json audit workflows
fr --json audit boundaries
```

Each report uses schema `fr-completion-audit-1` and carries a Merkle object root. The summary reports
parser, capability, workflow, recipe and application counts. Seven bounded detail sections expose
the underlying schemas, commands, source policy, proof classes and refusal reasons.

Keep these claims separate:

- A support predicate says whether a route admits a language, adapter or feature class.
- An acceptance target identifies an executable repository test.
- A retained evaluation records one fixed run.
- A proof names a model, its assumptions and its checked correspondence boundary.

An unrun test remains an obligation. A supported route can refuse a particular input. A model
theorem does not establish runtime behavior or general source equivalence.

## Check the guided workflows

`tools/completion-workflows.py` exercises understanding, tracing, direct changes, recipes, semantic
edits, framework migration and proof guidance on an isolated fixture. It executes at least one
returned read or preview action from each route and verifies that previews preserve source bytes.

```sh
python3 tools/completion-workflows.py --fr target/debug/fr --output /tmp/completion.json
python3 tools/completion-workflows.py --audit /tmp/completion.json
```

The committed report binds its evaluator and implementation sources by SHA-256. The auditor refuses
stale bindings, missing workflow families, absent action execution and overstated context claims.

## Interpret live-agent evidence

Live evaluators freeze a binary, isolate project copies and retain prompts, tool events, settings,
usage, source identities and scores. They require an explicit spend flag. Replay and scoring do not
contact the model service.

The accepted completion cohort proves that its two agents followed all seven guide families on its
fixture. The matched cohort compares local Python SDK execution with direct file reads for the same
seven preview outcomes. These runs do not establish general autonomous correctness or a population
context effect.

See [evaluation evidence](evaluations.md) for the current result, immutable manifests, reproduction
commands and interpretation limits.
