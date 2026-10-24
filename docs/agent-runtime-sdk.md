# Use fr from a Python agent program

Use the Python client when your agent needs to keep reports, inspect a few fields, or resume a reviewed change.
For example, store the callers of `render` in Python and show the agent only the caller it needs to inspect next.

SDK means software development kit; here it is a Python library for calling the `fr` command.
`FrClient` selects the project. A report is the returned data, and a pointer such as `/diff` selects a field within it.
The examples assume `fr` is installed and the Python package is available in the interpreter running the program.
For a repair, keep the report in Python, print the complete proposed diff, review it, then execute that reviewed change.
Keeping intermediate results outside the conversation still incurs Python execution, memory and validation costs.

For repeated Python questions, [save checked startup preparation](save-python-startup.md) between requests.
That complete lesson supplies every analysis setting explicitly and shows how source edits trigger a fresh check.

## Commands, results and limits

Start a structured task with `FrClient.guide(AgentGoal(...))` when you need help choosing a command.
The guide returns actions that read code or preview a change, with the fields the agent must supply.
`follow_guide` checks that the guide still applies before running an action and returning its report.
See the
[goal and workflow contract](agent-workflow-guide.md) for schemas, proof boundaries and complete
guided-versus-manual process and byte accounting.

For authored actions, pass `GuideInputs` and use `GuideFile` for recipe, property or tactics
content. `complete_guide` follows the full preview sequence inside one local Python program, keeping
intermediate reports out of the agent transcript. It requires an exact value for every named author
field, rechecks the guide basis before each action and never adds a write flag.

The zero-dependency Python package can retain `fr` reports as data instead of copying command JSON
through an agent's conversation. `FrClient` runs the local binary without a shell, fixes one project
root, bounds arguments, input, output and time, and returns `FrReport` objects. The Rust binary
remains the authority for project construction, handles, authoring, checks and history.

Install `fun-refactor-ir` from the same GitHub release as the native binary. At session startup,
`client.compatibility()` checks the exact distribution version, protocol revision and request and
response schema sets. It refuses mismatched artifacts before project work begins. Release CI builds
the wheel and source archive, installs the wheel into an isolated environment and checks it against
the release binary.

For this lookup, run from a project folder with one function named `render`.
For a complete starting file and installation, use the [Python package guide](../sdk/python/README.md).
The later fragments reuse `client` and returned targets; choose the operation your task needs.

```python
from fr_ir.runtime import FrClient

client = FrClient(".", executable="fr")
found = client.project("find", "render", "--signature")
target = found.definition_target()
```

`at` accepts an RFC 6901 pointer and returns only the selected detached Python value. `to_data`
returns the complete detached object when a local program needs it. Neither method prints the
report. A failed command raises `FrRuntimeError` with its argument tuple, exit status and structured
error report when one was returned.
`definition_targets()` reads tabular `find`, `select` and `map` rows, object-shaped `explore` rows
or the node returned by `show`. It checks the report revision against every handle, parses each exact
AST location and checks the reported coordinates before returning typed `AgentTarget` values. A map must
request the `location` field. Rows without a definition location, such as directories, remain
outside the result. `definition_target()` requires exactly one definition, so an absent or ambiguous
lookup refuses instead of choosing an arbitrary row.

## Progressive reveal

Start a semantic, evidence or project view with the same server-enforced profile and token limits as
the CLI. The response rejects a missing or inconsistent budget locally. `actions()` extracts only
exact `project disclose --reveal` continuations emitted by `fr`; optional domain filtering lets local
code choose one branch without showing every report to the model.

```python
initial = client.disclose(target.handle, view="evidence", token_limit=4096)
action = initial.actions(domain="project-evidence")[0]
revealed = client.follow(action)
```

`follow` accepts a `DisclosureAction`, not an arbitrary argument array. The action constructor also
checks the project-disclosure command shape. Rust still checks its revision, view, commitment, hole
identity and cursor. Source remains behind its own action and is not fetched by these calls unless
local code deliberately selects it.

For multi-page sections, use the [agent context workspace](agent-context-workspace.md). It binds all
responses to one disclosure identity, recursively materializes `code_map`, `call_traces`, `impact`,
`sources_and_sinks` or another selected pointer, verifies its Merkle digest and emits one bounded
packet. This avoids making an agent implement page traversal itself.

For a complete high-level request, declare an `AgentIntent`. Its purpose expands to stable evidence
sections. `compile` asks native `fr` to select them from one project snapshot and returns one packet:

```python
from fr_ir.intent import AgentIntent, IntentNeed

intent = AgentIntent(
    handle,
    "trace",
    needs=(
        IntentNeed("map", "code_map"),
        IntentNeed("calls", "call_traces"),
        IntentNeed("flows", "sources_and_sinks"),
    ),
)
compiled = client.compile(intent)
calls = compiled.at("/selected/calls")
```

The built-in purposes are `understand`, `trace`, `change`, `migrate` and `prove`. An intent accepts
at most 32 named projections across four current evidence sections and uses 64 calls at most per
section and 512 across the complete request, and returns at most 64 KiB. The default 192-call
budget covers each current built-in purpose. An explicit RFC 6901 suffix can select a smaller value
inside a section. Native compilation uses one `fr` process and reports zero progressive calls.
`client.prepare(intent)` follows each returned reading action for protocol tests and
independent agreement checks. Passing an object store to `compile` verifies and stores each selected
Merkle subtree under the digest returned by Rust.

A complete `change` intent can retain the reviewed action in the same packet. Wrap one direct
`TaskChange` with `IntentAction`; its only target must be the intent target and it cannot carry
project requests:

```python
from fr_ir.intent import AgentIntent, IntentAction

compiled = client.compile(AgentIntent(
    handle,
    "change",
    packet_limit=65_536,
    action=IntentAction(change),
))
# Review compiled.at('/action/review/author/diff').
result = client.execute_intent(compiled)
assert result.passed
```

The compiled value retains the exact canonical manifest, preview digest and `fraa1:` basis.
`execute_intent` checks those local identities before native `fr` rebuilds the review. Changed
source, intent, action, checks, required conditions after the edit or delivery options refuse before history creation.
The accepted route uses the same checked reversal and patch lifecycle as `TaskChange`.

## Complete reviewed changes

Every writable route uses the same `GuideReview`. Author one typed operation admitted by the
guide's `intent_action.operation_kinds`, create its native review, inspect the complete diff and
execute that unchanged review.

```python
from fr_ir.intent_actions import TaggedIntentAction

guide = client.guide(goal)
review = client.review_guide(guide, TaggedIntentAction(operation))
print(review.at("/diff"))
result = client.execute_guide(review)
```

The common admission check covers built-in capabilities, recipes, semantic task changes and
batches, surface edits, migrations, formal scaffolds and proof submissions. It requires exact goal,
guide, authored-input, target, revision, check, delivery and preview identities. Read-only
operations refuse execution.

A complete semantic scalar guide exposes `guide.semantic_scalar_action()`. It returns the typed
task operation only after matching the guide's ready manifest to the goal, target, checks, delivery
and required conditions after the edit, so the agent need not restate the same manifest.

The existing `TaskChange`, `TaskTarget` and `TaskDelivery` types build the wire object. `review`
serializes it once into canonical UTF-8 (the text encoding used to count source bytes) bytes and passes those bytes on standard input. A
`TaskReview` requires a ready non-executed preview, the exact manifest SHA-256 and a well-formed
`frtc1` basis.

```python
from fr_ir.ir import TaskChange, TaskDelivery, TaskTarget

change = TaskChange(
    [],
    [TaskTarget("render-body", handle, "replace-body",
                fragment="{ value.to_uppercase() }")],
    {"files-changed": 1, "edits": 1, "changed-operations": 1,
     "paths-changed": ["src/lib.rs"]},
    ["unit"],
    TaskDelivery(patch="artifacts/change.patch"),
)
review = client.review(change)
# Inspect review.at('/author/diff') and other selected fields here.
result = client.execute(review)
assert result.passed
```

Execution recomputes the retained manifest and preview identities before invoking `fr`. The binary
then recomputes the complete task-change basis from current project, target, fragment, check and
delivery state. Drift refuses before mutation. On acceptance, the existing lifecycle performs the
configured original check, apply, checks, reversal, restored check, redo, final check and patch
delivery.

`FrKernels.AgentSession` models the Python-facing draft, review and execution gate. Lean proves that
an execution needs the reviewed state, a valid unchanged preview, the same manifest and the same
basis; a draft cannot skip review. Rust and Lean agree on all 48 finite inputs, and Python and Rust
agree on the same corpus. These proofs cover the admission policy. Python's interpreter,
serialization and subprocess module, Rust JSON parsing, SHA-256, project construction, checks,
history and the filesystem remain trusted or integration-tested components.

The runtime does not replace the portable skill or create an ambient daemon. It is an optional
local orchestration layer for agents that can run Python. Shell-only agents can continue using the
same JSON protocol and exact actions.

## Controlled comparison

The baseline exposes each request and reply separately to the agent.
The compared approach runs those same requests inside one Python program and sends back its selected result.
This measures conversation bytes. It does not time Python startup, report validation or the tool's internal execution.

The [current Rust example](../tests/agent-eval/results/2026-10-21-final-ci-agent-runtime-context/result.json)
performs the same five internal `fr` calls in both arms: find, initial
evidence disclosure, one exact reveal, task-change preview and task-change execution. Both arms
produce the same final source and patch SHA-256, eight passed apply, check, reversal and delivery stages and applied history
state.

The direct JSON arm exposes five request/response exchanges totaling 18,136 bytes. The runtime arm
exposes its complete 1,480-byte Python program request and 422-byte result in one exchange, while the four
intermediate reports stay inside that local process. Text sent to the agent is 1,902 bytes, an 89.5%
reduction in this fixed task. Internal work remains five `fr` calls in both arms.

```sh
python3 tools/agent-runtime-context.py --fr target/debug/fr \
  --output /tmp/fr-runtime-context.json
python3 tools/agent-runtime-context.py --audit /tmp/fr-runtime-context.json
```

The measurement includes the complete program rather than treating SDK orchestration as free. It
does not run a model or measure hidden reasoning, tokens, billed quota, adoption or a population.

The [current single-request comparison](../tests/agent-eval/results/2026-10-21-final-ci-native-intent-context/result.json)
asks for the same three sections in both approaches. Both return the same selected information.
Following report pages launches `fr` 111 times and receives 391,001 reply bytes.
One native request launches `fr` once and receives 5,545 bytes.
Its final result grows from 4,480 to 5,545 bytes because it adds coverage information: what the tool examined and whether anything remains omitted.
These counts apply to this prescribed example. They do not measure elapsed time, memory, tokens, billed quota or results across other tasks.

```sh
python3 tools/native-intent-context.py --fr target/debug/fr > /tmp/fr-native-intent-context.json
python3 tools/native-intent-context.py --audit /tmp/fr-native-intent-context.json
```
