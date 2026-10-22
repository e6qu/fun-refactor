# fun-refactor

`fr` is a command-line tool that helps an AI coding agent find code, review a change and check the result.
It can read selected functions and callers, preview edits, run declared checks and undo saved changes.

Suppose you ask an agent to fix a rounding error in checkout. It can use `fr` to find the calculation,
inspect its callers, prepare a patch and run the test that catches the error. You can review the diff
and reverse a saved change if it is wrong.

The larger goal is to finish unfamiliar tasks with less reading, fewer retries and lower cost.
We have useful component tests, but have not demonstrated that broader benefit. Python value tracing
works for supported examples; imports and other behavior in real libraries still often leave an unknown answer.
Read [where we are going](PLAN.md) or the [current results](docs/roadmap-status.md) before relying on a stronger claim.
The [measurement guide](docs/benchmarks.md) explains what we compare, the extra work introduced by `fr`, and what remains unmeasured.

## Install

Download the archive and matching `.sha256` file from
[GitHub Releases](https://github.com/e6qu/fun-refactor/releases/latest).

| Archive | Platform |
|---|---|
| `fr-<tag>-x86_64-unknown-linux-musl.tar.gz` | Linux amd64 |
| `fr-<tag>-aarch64-unknown-linux-musl.tar.gz` | Linux arm64 |
| `fr-<tag>-x86_64-apple-darwin.tar.gz` | macOS Intel |
| `fr-<tag>-aarch64-apple-darwin.tar.gz` | macOS Apple silicon |
| `fun-refactor-<tag>-wasm.tar.gz` | Browser or Node |
| `fun_refactor_ir-<version>-py3-none-any.whl` | Python agent SDK |

Verify, unpack and install. Linux users can use `sha256sum -c` in place of `shasum`.

```sh
shasum -a 256 -c fr-*.tar.gz.sha256
tar -xzf fr-*.tar.gz
mkdir -p "$HOME/.local/bin"
install "$(find . -path '*/fr' -type f | head -n 1)" "$HOME/.local/bin/fr"
fr --version
```

To build the current source, install stable Rust and run:

```sh
git clone https://github.com/e6qu/fun-refactor.git
cd fun-refactor
cargo install --locked --path .
fr --version
```

## First use

Start with the [small, complete tutorial](TUTORIAL.md). It creates its own example files and explains each new term.
You need an installed `fr` binary and Python 3 to follow that lesson.

A project root is the folder containing the project you want to inspect.
Run `fr` there. Add `--json` to return structured data for another program.
A preview shows proposed changes without applying them. Commands that edit source preview by default.
For an existing project, these commands list files, check readable syntax and find functions:

```sh
fr scan
fr parse --stats
fr symbols --kind function
fr project map --depth 2 --limit 30
```

A rename changes a declaration and the uses that `fr` can identify.
Replace `old_name` and `new_name` below with your current and requested function names.
Review the preview before saving it. A saved change forms a transaction; its ID identifies it in history.

```sh
fr rename old_name new_name
fr rename old_name new_name --save-plan
fr history show '<TX>'

fr history apply '<TX>' --write
fr history undo '<TX>' --write
fr history redo '<TX>' --write

fr history patch '<TX>' --output change.patch
```

Replace `<TX>`, including its brackets, with the returned transaction ID. Continue with the [tutorial](TUTORIAL.md), or use the
[CLI reference](CLI.md) for the complete command surface.

## Give `fr` to an agent

This checkout includes project skills for [Claude Code](.claude/skills/fr)
and [Codex and OpenCode](.agents/skills/fr).
Both entries point to the same maintained [fr skill](skills/fr/SKILL.md), including its references.
Put `fr` on the agent's `PATH`, then load the skill and follow [your first checked change](skills/fr/references/first-change.md).

To make the skill available in other projects, copy the complete `skills/fr` directory
from the source tree or release archive into your agent's user skill directory:

| Agent | User skill directory |
| --- | --- |
| Claude Code | `~/.claude/skills/fr/` |
| Codex | `~/.agents/skills/fr/` |
| OpenCode | `~/.config/opencode/skills/fr/` |

The [installation guide](docs/agent-skill.md#install-the-handoff) gives commands and explains how to check discovery.
Keep [`SKILL.md`](skills/fr/SKILL.md) and its adjacent `references` directory together.
The skill routes a task to a small relevant guide rather than loading the whole manual.

Start with the live support summary and a structured goal:

`audit` needs no input file. The second command needs a task description saved as `goal.json`;
the [goal guide](docs/agent-workflow-guide.md) explains its fields and supplies an example.

```sh
fr --json audit
fr --json guide --from goal.json
```

For a bug fix, the agent should:

1. Check what the installed tool supports with `fr audit`.
2. Find the relevant function and inspect its callers.
3. Read the source needed to explain the mistake.
4. Preview the proposed change and review the whole diff.
5. Apply that same reviewed change and run the relevant behavior checks.
6. Keep the saved change or patch so you can inspect, resume or undo it.

`fr guide` can suggest these steps from a structured task description. The agent supplies that
description; the tool does not interpret the user’s natural-language request on its own.

The optional Python client can keep a large report in memory and send the agent only the section
it needs. For example, send the callers of `calculate` without repeating the whole project map.
Its constructors also check the shape of edits before sending them to `fr`.
Download the wheel carrying the same version as `fr`, then install and verify the pair:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install fun_refactor_ir-<version>-py3-none-any.whl
.venv/bin/python -c 'from fr_ir.runtime import FrClient; print(FrClient(".").compatibility().version)'
```

From a source checkout:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install ./sdk/python
```

This example prepares a rename, shows its diff and runs the declared `unit` check after execution.
The project must already declare that check in `.fr/checks.json`.
Review the printed diff before running the final two lines:

```python
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.intent_actions import CapabilityOperation, TaggedIntentAction
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient

client = FrClient(".")
delivery = TaskDelivery(patch="artifacts/change.patch")
goal = AgentGoal(
    "change",
    selector=GoalSelector(name="calculate"),
    operation=GoalOperation("capability", {
        "capability": "rename", "parameters": {"new_name": "evaluate"},
    }),
    checks=("unit",),
    delivery=delivery,
)

guide = client.guide(goal)
review = client.review_guide(guide, TaggedIntentAction(CapabilityOperation(
    "rename", {"new_name": "evaluate"}, checks=("unit",),
    delivery=delivery,
)))
print(review.at("/diff"))
result = client.execute_guide(review)
assert result.passed
```

See the [agent quickstart](docs/agent-skill.md), [goal and workflow contract](docs/agent-workflow-guide.md),
[Python runtime](docs/agent-runtime-sdk.md) and [SDK reference](sdk/python/README.md).

## Common agent tasks

Inspect one symbol and its local relationships:

```sh
fr --json project find render --signature
fr --json project show '<HANDLE>' --relations --limit 8

fr --json project calls '<HANDLE>' --direction incoming --limit 8
fr --json project tests '<HANDLE>' --limit 8
```

Explore an unfamiliar subsystem without loading source:

```sh
fr --json project map --depth 3 --limit 40
fr --json project explore payment --contains

fr --json project disclose '<HANDLE>' --view evidence --depth 3 --token-limit 4096
```

Preview a high-level multi-file change:

```sh
fr --json project find calculate --in src/lib.rs --source --bytes 2048
fr --json author guide

fr --json author batch --from change.json
fr --json author batch --from change.json --save-plan --plan-basis '<PLAN_BASIS>'
```

Inspect and migrate an admitted framework feature:

```sh
fr --json project features --limit 12
fr --json project features --feature '<FEATURE_ID>'
fr --json migrate feature '<FEATURE_ID>' --to fastapi --out services/route.py
```

Generate an application from an admitted application IR bundle:

```sh
fr --json project application
fr --json migrate application --project . --to express --out generated
```

Agents can also author required query and JSON-body scalar validation through the typed Python
application IR. `project application` reads the documented source subsets from Next.js, FastAPI,
Express and Go HTTP back into that IR. See the [application IR contract](docs/application-ir.md)
for exact reader, canonicalization and writer boundaries.

Create and check a project-specific Lean proof package:

```sh
fr --json spec candidates src
fr --json spec property-task src/lib.rs::allowed
fr --json spec plan src/lib.rs::allowed --property-from property.json
fr --json spec scaffold --from formal-plan.json --write
fr --json spec goals specs
fr --json spec proof-task specs/FrSpecs/Model.lean::obligation
fr --json spec proof-check specs/FrSpecs/Model.lean::obligation --from proof.lean
fr --json spec prove specs/FrSpecs/Model.lean::obligation --from proof.lean --write
fr --json spec verify specs
```

## Scope and guarantees

`fr` can read 19 source formats.
The source formats are Rust, Go, Zig, Java, TypeScript, TSX, Python, Bash, HTML, CSS, SCSS,
Sass, HCL, JSON, YAML, Helm, XML, Markdown and Lean. JavaScript uses the TypeScript grammar and JSX
uses TSX. React, Next.js, Express, FastAPI and Tailwind are framework surfaces.

Run these commands for current support instead of relying on a copied matrix:

```sh
fr capabilities
fr --json audit
fr --json audit frameworks
fr --json audit proofs
```

`fr` preserves original bytes outside selected edit ranges and reparses every edited file. It
reports ambiguity, incomplete parsing, weak call edges, omissions and unsupported constructs rather
than converting them into certainty. Compilation and behavior require declared checks or another
explicit oracle.

Finding a route in source does not establish how its authentication or middleware behaves at runtime.
Test the behavior that matters to your change. A Lean proof concerns its stated model and assumptions;
check the connection to your function before treating it as a guarantee about the code.

## Documentation

The [documentation map](docs/README.md) groups material for users, agent integrators and
contributors. The main references are:

- [Tutorial](TUTORIAL.md)
- [CLI reference](CLI.md)
- [Agent workflow](docs/agent-workflow-guide.md)
- [Progressive disclosure](docs/progressive-disclosure.md)
- [Semantic model](docs/semantic-model.md)
- [Application IR](docs/application-ir.md)
- [Formal verification](docs/lean-specs.md)
- [Roadmap](PLAN.md)
- [Known defects](BUGS.md)

## Develop

The [development guide](docs/development.md) explains how to build, test and contribute.
On the shared workstation, read its resource rules first. Full builds and complete checks
run on GitHub; a refused local guard is a reason to use CI, not to raise the limits.

## Licence

AGPL-3.0-or-later.
