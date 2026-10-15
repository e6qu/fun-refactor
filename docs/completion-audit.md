# Check what fr can do before starting

Before asking an agent to trace Python input or migrate a route, check whether the installed tool supports the required operation.
`fr audit` lists its support and limits. A supported operation can still refuse a particular input; read that reason before choosing the next step.

A capability is an operation the installed tool advertises. A boundary describes a limit on what it can establish.
`audit` reports those capabilities; it does not run your project's tests.
For an imported Python helper, check import support before requesting a trace, then inspect the actual trace's unfinished parts.
“Supported” describes a route that may accept suitable input, not a guarantee that your whole project fits it.

## Commands, results and limits

`fr audit` gives an agent a compact support and trust summary before it chooses a workflow. The
summary uses the same support rules as the installed commands.

```sh
fr audit
fr --json audit workflows
fr --json audit boundaries
```

Each report uses schema `fr-completion-audit-1` and carries a Merkle object root. The summary reports
parser, capability, workflow, recipe and application counts. Seven bounded detail sections expose
the underlying schemas, commands, source policy, proof classes and refusal reasons.

Read each kind of entry according to what it tells you:

| Entry | Example | Your next action |
| --- | --- | --- |
| Supported operation | Python body replacement is available | Preview your function and read any refusal |
| Test name | A repository check covers reversal | Run the relevant check on the revision you intend to use |
| Recorded experiment | A fixed patch passed on a chosen task | Read that task and the limits before generalizing |
| Model theorem | A rule requires an unchanged review | Check which model it proves and how that connects to the code |

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
