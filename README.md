# fun-refactor

`fr` gives agents bounded, structured access to a codebase and applies reviewed changes with checks,
undo, redo and Git patch delivery. It supports 19 parser identities in one standalone binary and
does not require a language server, daemon or project configuration for ordinary use.

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

Run `fr` from a project root. Add `--json` for an agent or program. Mutating commands preview by
default and write only through an explicit reviewed path.

```sh
fr scan
fr parse --stats
fr symbols --kind function
fr project map --depth 2 --limit 30
fr rename old_name new_name
```

Save an accepted preview to durable history, then apply, reverse or export it:

```sh
fr rename old_name new_name --save-plan
fr history show '<TX>'

fr history apply '<TX>' --write
fr history undo '<TX>' --write
fr history redo '<TX>' --write

fr history patch '<TX>' --output change.patch
```

`<TX>` is the returned transaction ID. Continue with the [tutorial](TUTORIAL.md), or use the
[CLI reference](CLI.md) for the complete command surface.

## Give `fr` to an agent

Put `fr` on the agent's `PATH` and provide the complete `skills/fr` directory from the source tree
or release archive. For Codex:

```sh
mkdir -p "$HOME/.codex/skills"
cp -R skills/fr "$HOME/.codex/skills/fr"
```

Other agents can load [`skills/fr/SKILL.md`](skills/fr/SKILL.md) with its adjacent `references`
directory. The skill routes a task to a small relevant guide rather than loading the whole manual.

Start with the live support summary and a structured goal:

```sh
fr --json audit
fr --json guide --from goal.json
```

An agent should:

1. Ask `fr audit` or the skill what is supported.
2. Express an `understand`, `trace`, `change`, `migrate` or `prove` goal.
3. Follow the exact bounded actions returned by `fr guide`.
4. Reveal source only when semantic or project evidence is insufficient.
5. Author the requested scalar, typed IR, recipe, migration choice or Lean tactics.
6. Review the complete preview and retain its basis.
7. Execute only the unchanged review, run declared checks and keep the transaction or patch.

The optional Python SDK keeps intermediate reports outside the model transcript and mirrors public
IR shapes with typed objects. From a source checkout:

```sh
python3 -m venv .venv
.venv/bin/python -m pip install ./sdk/python
```

This example performs a checked semantic change through one retained guide run:

```python
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient

client = FrClient(".")
goal = AgentGoal(
    "change",
    selector=GoalSelector(name="calculate"),
    operation=GoalOperation(
        "semantic-scalar",
        {"operation": "set-int", "from": "7", "to": "9"},
    ),
    checks=("unit",),
    delivery=TaskDelivery(patch="artifacts/change.patch"),
)

run = client.complete_guide(goal)
print(run.review().at("/author/diff"))
result = client.execute_guide(run)
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

The parser identities are Rust, Go, Zig, Java, TypeScript, TSX, Python, Bash, HTML, CSS, SCSS,
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

Static framework evidence does not prove arbitrary middleware, authentication, service behavior,
dynamic rendering or runtime-generated routes. Lean theorems establish their named model properties
under stated assumptions; source correspondence requires a checked bridge or bounded execution
evidence.

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

The [development guide](docs/development.md) covers toolchains, tests, Lean resource limits,
grammar provenance and releases.

```sh
cargo build --locked
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

## Licence

AGPL-3.0-or-later.
