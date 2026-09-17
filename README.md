# fun-refactor

`fr` finds and changes code across 19 languages. It is one standalone binary: no language server,
daemon or project configuration is required for ordinary analysis and refactoring.

## Install

Download the latest archive and matching `.sha256` file from
[GitHub Releases](https://github.com/e6qu/fun-refactor/releases/latest), then choose the target for
your machine.

| Archive | Machine |
|---|---|
| `fr-<tag>-x86_64-unknown-linux-musl.tar.gz` | Linux, amd64 |
| `fr-<tag>-aarch64-unknown-linux-musl.tar.gz` | Linux, arm64 |
| `fr-<tag>-x86_64-apple-darwin.tar.gz` | macOS, Intel |
| `fr-<tag>-aarch64-apple-darwin.tar.gz` | macOS, Apple silicon |
| `fun-refactor-<tag>-wasm.tar.gz` | Browser or Node module |

Verify, unpack and install the archive. Linux users can substitute `sha256sum -c` for the first
command.

```sh
shasum -a 256 -c fr-*.tar.gz.sha256
tar -xzf fr-*.tar.gz
mkdir -p "$HOME/.local/bin"
install "$(find . -path '*/fr' -type f | head -n 1)" "$HOME/.local/bin/fr"
fr --version
```

To build the current source instead, install a stable Rust toolchain and run:

```sh
git clone https://github.com/e6qu/fun-refactor.git
cd fun-refactor
cargo install --locked --path .
fr --version
```

The Linux release binaries are static. The native release archive also includes `README.md`,
`CLI.md`, the licence and the portable agent skill.

## First five minutes

Run these commands from a project root. Read commands return text by default; add `--json` for an
agent or program. Mutations preview a diff until `--write` is present.

```sh
fr scan
fr parse --stats
fr symbols --kind function
fr project map --depth 2 --limit 30
fr rename old_name new_name
```

A preview that looks correct can be saved and applied through durable history:

```sh
fr rename old_name new_name --save-plan
fr history show '<TX>'

fr history apply '<TX>' --write
fr history undo '<TX>' --write

fr history redo '<TX>' --write
fr history patch '<TX>' --output change.patch
```

`<TX>` is the transaction ID returned by `--save-plan`. See the
[tutorial](TUTORIAL.md) for a complete walkthrough and the [CLI reference](CLI.md) for every command.

## Set up an agent

Make `fr` available on the agent's `PATH`, then give it the complete `skills/fr` directory from the
release archive or this repository. For Codex, copy the skill into the user skill directory:

```sh
mkdir -p "$HOME/.codex/skills"
cp -R skills/fr "$HOME/.codex/skills/fr"
```

For another agent, point its skill or instruction loader at `skills/fr/SKILL.md` and keep the
adjacent `references` directory. The skill routes each task to a small relevant reference instead
of loading the whole manual.

Start an agent session with the live support summary, then let `fr` turn structured intent into
bounded read and preview actions:

```sh
fr --json audit
fr --json guide --from goal.json
```

The agent should follow this sequence:

1. Read `fr audit` or the portable skill to discover live support and trust boundaries.
2. Express the task as `fr-agent-goal-1`, or select a focused project query directly.
3. Follow only the exact actions returned by `fr guide`; reveal source only when structured evidence
   is insufficient.
4. Author required values, fragments, semantic IR, migration choices or Lean tactics.
5. Review the complete preview and retain its revision and basis.
6. Execute the unchanged reviewed plan through history, `task-change` or `workflow`.
7. Run declared checks, export a patch, and keep the transaction ID for undo, redo or recovery.

The optional Python SDK keeps intermediate reports outside the model transcript and mirrors the
wire IR with typed Python objects. From a source checkout:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install ./sdk/python
```

For a checked scalar change, the agent can keep discovery and preview reports local, inspect one
reviewed diff, then execute that unchanged run:

```python
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient

client = FrClient(".")
goal = AgentGoal(
    "change",
    selector=GoalSelector(name="calculate"),
    operation=GoalOperation("semantic-scalar", {
        "operation": "set-int", "from": "7", "to": "9",
    }),
    checks=("unit",),
    delivery=TaskDelivery(patch="artifacts/change.patch"),
)

run = client.complete_guide(goal)
review = run.review()
print(review.at("/author/diff"))

result = client.execute_guide(run)
assert result.passed
```

Use the [agent quickstart](docs/agent-skill.md), [workflow guide](docs/agent-workflow-guide.md),
[Python runtime](docs/agent-runtime-sdk.md) and [Python package README](sdk/python/README.md) for the
complete contracts.

## Agent scenarios

### Inspect a known symbol

Find the exact revision-bound handle, then request only the relationships needed by the task.

```sh
fr --json project find render --signature
fr --json project show '<HANDLE>' --relations --limit 8

fr --json project calls '<HANDLE>' --direction incoming --limit 8
fr --json project tests '<HANDLE>' --limit 8
```

### Explore an unfamiliar subsystem

Begin with a shallow hierarchy or bounded name search. Use progressive disclosure for code maps,
call traces, impact, and local sources and sinks without loading source text.

```sh
fr --json project map --depth 3 --limit 40
fr --json project explore payment --contains

fr --json project disclose '<HANDLE>' --view evidence --depth 3 --token-limit 4096
```

### Perform a built-in refactor

Preview first. Save the exact plan, apply it, run declared checks and export the reviewed patch.

```sh
fr rename old_name new_name
fr rename old_name new_name --save-plan

fr history apply '<TX>' --write --no-diff
fr checks

fr checks --run unit --basis '<CHECK_BASIS>' --quiet-success --no-declarations
fr history patch '<TX>' --output change.patch
```

### Author a high-level code change

Select exact declarations with bounded source, inspect the authoring schema, then preview a batch.
The batch can coordinate bodies, signatures, callers and imports in one transaction.

```sh
fr --json project find calculate --in src/lib.rs --source --bytes 2048
fr --json author guide

fr --json author batch --from change.json
fr --json author batch --from change.json --save-plan --plan-basis '<PLAN_BASIS>'
```

For source-free changes, use semantic bodies, pointer deltas, scalar intents, or disclosed edit
capabilities described in the [semantic model](docs/semantic-model.md) and
[disclosed editing](docs/disclosed-editing.md).

### Migrate a framework feature

Inspect the application hierarchy, select one revision-bound feature, then preview a compatible
Next.js or FastAPI route migration. Unsupported runtime behavior remains an explicit gap.

```sh
fr --json project features --limit 12
fr --json project features --feature '<FEATURE_ID>'
fr --json migrate feature '<FEATURE_ID>' --to fastapi --out services/route.py
```

### Recover or reverse a change

Use source history for refactor and author transactions. Recovery is only for a reported interrupted
operation.

```sh
fr history
fr history show '<TX>'
fr history undo '<TX>' --write --no-diff
fr history redo '<TX>' --write --no-diff
fr history recover '<TX>' --write
```

### Build and check a Lean proof

`fr` generates bounded models and proof tasks; the agent writes the property and tactics. A checked
Lean model proves its proposition under its definitions and assumptions, while implementation
correspondence remains separate evidence.

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

## What `fr` provides

| Need | Main surface |
|---|---|
| Cross-language structure and references | `symbols`, `def`, `refs`, `project map`, `project find` |
| Calls, value flow, impact and configuration provenance | `callers`, `callees`, `flow`, `impact`, `stitch` |
| Built-in refactoring and translation | `rename`, `delete`, `extract`, `inline`, `signature`, `move`, `translate` |
| High-level authored changes | `author`, semantic IR, disclosed capabilities, recipes |
| Framework understanding and migration | `project features`, application IR, `migrate feature` |
| Progressive context | `project disclose`, Merkle objects, bounded continuations |
| Verification | declared checks, behavioral workflows, Lean models and proof tasks |
| Reversibility and delivery | history, undo, redo, recovery, Git patches, staging and commits |
| Agent integration | `audit`, `guide`, portable skill and zero-dependency Python SDK |

`fr` parses with tree-sitter and preserves original bytes outside selected edit ranges. It reports
coverage, uncertainty and omissions instead of turning an unresolved case into certainty. Ambiguous
targets refuse with candidates. Every edited file is reparsed before commit; compilation and
behavior still require declared checks or another explicit oracle.

## Languages and capability matrix

JavaScript uses the TypeScript grammar for `.js`, `.mjs` and `.cjs`; JSX uses the TSX grammar. Run
`fr capabilities` for the reason behind every unsupported or inapplicable cell.

| Capability | rust | go | zig | java | typescript | tsx | python | bash | html | css | scss | sass | hcl | json | yaml | helm | xml | markdown | lean |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| symbols/def/refs | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| rename | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| safe delete | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| impact | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| restructure | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| call graph | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ |
| flow | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ |
| provenance | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a |
| entry points | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | ✓ | n/a | n/a | ✓ | ✓ | ✓ | n/a |
| extract variable | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a |
| extract function | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ | n/a | n/a | n/a | ✓ | n/a | n/a | n/a |
| inline variable | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a |
| inline call | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| change signature | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | ✓ |
| micro-rewrites | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| organize imports | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| remove flag | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | ✓ | n/a | n/a | n/a | n/a | n/a | n/a |
| move to file | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ |
| config→code stitch | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | ✓ | ✓ | n/a | n/a | n/a |
| duplicate code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| dead code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| write as another language | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| declared HTTP contract | n/a | n/a | n/a | n/a | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| declared type | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a | n/a |

The generated matrix records **311 of 456 capability × language pairs supported, 145 not applicable**.
Exact inputs can still refuse because of ambiguity, syntax, missing evidence or an unsupported
construct.

## Boundaries

- Reports identify string and comment occurrences but never rewrite them as resolved references.
- Weak or unresolved call and flow edges remain explicit uncertainty.
- A syntax-clean edit does not establish type safety or behavior.
- Static framework evidence does not model arbitrary middleware, authentication, dynamic rendering
  or runtime-generated routes.
- Translation and framework migration support the constructs admitted by their IR and refuse the
  rest with gaps.
- Lean theorems establish model properties. Source correspondence needs its own checked bridge or
  bounded execution evidence.

Read [known defects and boundaries](BUGS.md) for the maintained ledger.

## Documentation

The [documentation map](docs/README.md) groups user guides, agent workflows, contracts, verification,
evaluation evidence and contributor material. Useful starting points are:

- [Tutorial](TUTORIAL.md)
- [CLI reference](CLI.md)
- [Examples](EXAMPLES.md)
- [Cross-language behavior](CROSS_LANGUAGE.md)
- [Intermediate representation](IR.md)
- [Recipes](RECIPES.md)
- [HTTP contracts](API_CONTRACTS.md)
- [Roadmap](PLAN.md)
- [Changelog](CHANGELOG.md)

## Develop

The [development guide](docs/development.md) covers local checks, Lean resource limits, grammar and
query provenance, adding a language and the release process.

```sh
cargo build --locked
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

## Licence

AGPL-3.0-or-later.
