# Agent runtime SDK

Start unfamiliar structured tasks with `FrClient.guide(AgentGoal(...))`. The language-aware guide
selects an existing read/preview route and returns exact actions with only the required authored
fields. `follow_guide` revalidates freshness and output identity before returning local report data;
exact scalar goals with checks return the existing `TaskReview`. See the
[goal and workflow contract](agent-workflow-guide.md) for schemas, proof boundaries and complete
guided-versus-manual process and byte accounting.

The zero-dependency Python package can retain `fr` reports as data instead of copying command JSON
through an agent's conversation. `FrClient` runs the local binary without a shell, fixes one project
root, bounds arguments, input, output and time, and returns `FrReport` objects. The Rust binary
remains the authority for project construction, handles, authoring, checks and history.

```python
from fr_ir.runtime import FrClient

client = FrClient(".", executable="fr")
found = client.project("find", "render", "--signature")
handle = found.at("/rows/0/0")
```

`at` accepts an RFC 6901 pointer and returns only the selected detached Python value. `to_data`
returns the complete detached object when a local program needs it. Neither method prints the
report. A failed command raises `FrRuntimeError` with its argument tuple, exit status and structured
error report when one was returned.

## Progressive reveal

Start a semantic, evidence or project view with the same server-enforced profile and token limits as
the CLI. The response rejects a missing or inconsistent budget locally. `actions()` extracts only
exact `project disclose --reveal` continuations emitted by `fr`; optional domain filtering lets local
code choose one branch without showing every report to the model.

```python
initial = client.disclose(handle, view="evidence", token_limit=4096)
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
`client.prepare(intent)` retains the earlier action-by-action implementation for protocol tests and
independent parity checks. Passing an object store to `compile` verifies and stores each selected
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
source, intent, action, checks, postconditions or delivery options refuse before history creation.
The accepted route uses the same checked reversal and patch lifecycle as `TaskChange`.

## Complete reviewed changes

The existing `TaskChange`, `TaskTarget` and `TaskDelivery` types build the wire object. `review`
serializes it once into canonical UTF-8 bytes and passes those bytes on standard input. A
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

The retained generic fixture performs the same five internal `fr` calls in both arms: find, initial
evidence disclosure, one exact reveal, task-change preview and task-change execution. Both arms
produce the same final source and patch SHA-256, eight passed lifecycle stages and applied history
state.

The direct JSON arm exposes five request/response exchanges totaling 17,689 bytes. The runtime arm
exposes its complete 1,471-byte Python program and 422-byte result in one exchange, while the four
intermediate reports stay inside that local process. Agent-visible payload is 1,893 bytes, an 89.3%
reduction in this fixed task. Internal work remains five `fr` calls in both arms.

```sh
python3 tools/agent-runtime-context.py --fr target/debug/fr
python3 tools/agent-runtime-context.py --audit tests/agent-eval/agent-runtime-context.json
```

The measurement includes the complete program rather than treating SDK orchestration as free. It
does not run a model or measure hidden reasoning, tokens, billed quota, adoption or a population.

The native intent comparison runs the same three-section trace through both compilers. Both return
the same selected value digest. Progressive compilation uses 89 subprocesses and receives 302,380
internal response bytes; native compilation uses one subprocess and receives 4,975 bytes. The final
native packet is 4,975 bytes versus 3,909 for the locally assembled packet because it also carries
the project coverage envelope. These are deterministic process and byte measurements, not model,
token, quota or population results.

```sh
python3 tools/native-intent-context.py --fr target/debug/fr
python3 tools/native-intent-context.py --audit tests/agent-eval/native-intent-context.json
```
