# Language-aware agent workflow guide

`fr guide --from GOAL` selects a deterministic read/preview route from structured intent. It does
not interpret natural language or execute changes. The same `fr-agent-goal-1` data works through
`FrClient.guide` in Python.

```json
{
  "schema": "fr-agent-goal-1",
  "purpose": "change",
  "selector": {"name": "calculate", "scope": "src", "language": "rust"},
  "operation": {"kind": "semantic-scalar", "operation": "set-int", "from": "7", "to": "9"},
  "checks": ["unit"],
  "context": {"token_limit": 4096, "packet_limit": 16384},
  "delivery": {
    "check-original": true,
    "compact-success": true,
    "exercise-reversal": true,
    "patch": "artifacts/change.patch",
    "check-output-bytes": 256
  }
}
```

Use a full revision-bound `target` handle, or a selector with exactly one `name` or `path`. A name
selector can restrict `scope`, symbol `kind`, parser `language` and `locals`. No target means the
workspace hierarchy. Ambiguous names return at most eight candidates and an exact omission count;
they never choose an arbitrary declaration. Repeat the goal with one returned full handle.

The purposes are `understand`, `trace`, `change`, `migrate` and `prove`. `operation` defaults to
`{"kind":"automatic"}`. Automatic understanding/tracing selects bounded evidence; automatic changes
select semantic deltas; migration requires an advertised destination; proving selects the Rust
formalization workbench or a Lean proof task.
Automatic Lean declaration goals derive the obligation from its selected name; file goals still
need an exact obligation. Missing specification packages receive a non-writing initialization
preview before formal scaffold authoring.

| Operation kind | Agent-authored content | Existing authority |
|---|---|---|
| `capability` | `capability` and named scalar `parameters` | Live capability support and exact CLI forms |
| `recipe` | One live `verb`, then a recipe file | Recipe vocabulary, parser and runner |
| `semantic-scalar` | Live `operation`, exact `from` and new `to` | Semantic locator/intent and task planners |
| `semantic-change` | Basis-bound typed operations | Semantic change catalog and writer |
| `semantic-body` | Typed statement tree | Semantic body catalog and writer |
| `surface-edit` | `surface`: `styles` or `diagrams`, then returned edit ID and new scalar | CSS/host/Markdown/Mermaid capabilities |
| `framework-migration` | Advertised destination, feature or application output, and optional registration data | Feature or application IR compatibility |
| `formalize` | Property tree, then proof tactics | Conservative pure Rust formalization workbench |
| `proof` | Exact `obligation`, then tactics without `by` | Existing checked Lean proof-task loop |

Every `fr-agent-guide-1` response names the selected target, language and detected technology
evidence, route admission, refusals, uncertainty, reference, exact `arguments`, output contract,
optional stdin `input` and only the `author_fields` needed by each action. `ready` actions contain
complete inputs. Templates with author fields require those fields first. Returned arrays never
contain `--write` or `--save-plan`. Review and execution use the existing task, history, workflow,
migration or proof engine's complete basis.
Recipe guidance includes only its live verb, relevant selector fields, target values, support and
matched/refusal expectation forms. Target-kind and language incompatibility refuses before authoring.

Exact scalar goals with declared checks produce one complete `fr-task-change-1` preview input.
Integer/float scalars follow the live unsigned-decimal contract; negatives use explicit unary IR
nodes. Option-like string values use equals-form arguments so the CLI preserves their data role.
The native task planner checks uniqueness, scalar category, writer admission and postconditions
before the guide advertises it. Python returns the preview as `TaskReview`; the existing `execute`
method runs original checks, apply, checks, reversal, restored checks, redo, final checks and optional
patch delivery according to the supplied policy.

```python
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient

client = FrClient(".")
goal = AgentGoal(
    "change",
    selector=GoalSelector(name="calculate", scope="src", language="rust"),
    operation=GoalOperation("semantic-scalar", {"operation": "set-int", "from": "7", "to": "9"}),
    checks=("unit",),
    delivery=TaskDelivery(patch="artifacts/change.patch", check_output_bytes=256),
)
run = client.complete_guide(goal)
review = run.review()
print(review.at("/author/diff"))
result = client.execute_guide(run)
assert result.passed
```

Intermediate reports stay as local Python data. `at` reveals one detached field; it prints nothing.
`follow_guide` revalidates the goal basis before following a ready action, verifies its output
contract and enforces the response byte ceiling. Changed input, source, capability, schema, checks,
target or guide receipt refuses. Shell agents should revalidate guidance on the current revision
before following direct positional previews, then review the authoritative preview basis.

Actions that name `author_fields` accept the same wire-shaped values through `GuideInputs`.
Use `GuideFile` for recipes, formal properties, plans or Lean tactics; the runtime writes it to a
bounded private temporary file for the one preview call and removes it afterward. Scalar values
replace only their exact `<field-name>` argument. Missing, extra, duplicate, unbounded or
execution-like values refuse before the action runs.

```python
from fr_ir.guide import GuideFile, GuideInputs

run = client.complete_guide(goal, {
    1: GuideInputs({"tactics-file": GuideFile("proof.lean", "rfl\n")}),
    2: GuideInputs({"tactics-file": GuideFile("proof.lean", "rfl\n")}),
})
assert run.reports[-1].at("/applied") is False
```

`complete_guide` obtains one guide and follows every read or preview action locally. The caller
still authors every required value and reviews the returned reports. `GuideRun.review()` accepts
only a run with exactly one immutable task review. After that explicit review boundary,
`execute_guide` refreshes the guide again and executes only the unchanged retained task. Routes
with another write shape refuse and direct the caller to `compile_guided_intent`. Each action checks
the guide basis, output schema and byte ceiling. This form is intended for an agent-authored Python
program whose intermediate project reports should stay outside the model transcript.

The guide contains no source, semantic body, complete vocabulary or unrelated route. Exact source
requires `constraints.allow_source: true` and a separate bounded reveal. Reveal limits range from
1,024 through 4,096 conservative token upper bounds; guide packets range from 2 through 64 KiB.
`serialized_bytes` measures the complete compact JSON response. Its `object_root` commits to the
report before `basis`, `object_root` and the measured length; `frag1:` additionally binds the normalized
goal, project revision, all live capability forms/support, recipe vocabulary and semantic catalog.

The verification ladder keeps reparse, compiler, declared checks, behavioral oracles, model theorems
and implementation correspondence separate. A route starts with no proved behavior. Agents author
properties and tactics. Generated formalization admits typed pure declarations for the languages
listed by `fr --json audit proofs` and structural snapshots for the remaining parser identities. An
`implementation` proof expectation returns an explicit unsupported state.

Some older direct commands return unversioned JSON objects. Their guide actions have a null
`output_schema`; versioned commands identify their exact schema and `schema_field`, including
`semantic_schema` for minimal semantic reports. The navigator preserves this distinction.

`FrKernels.AgentGuide` proves matching purpose/live support/source permission requirements, exact
bounded author-field binding and the complete unchanged-review requirement for execution. Rust,
Python and executable Lean agree over finite route, lifecycle, binding and guided-delivery corpora.
These proofs cover the admission policies;
parsing, serialization, SHA-256, target construction, planners, writers, subprocesses, toolchains,
proof completeness and general implementation correspondence remain separate tested/trusted
boundaries.

The retained compatibility [complete-program comparison](../tests/agent-eval/agent-guide-context.json) changes a
generic Rust scalar, runs compiler and finite behavioral checks, emits an identical patch and
completes all eight lifecycle stages in both arms. The manual program and final packet occupy
1,155 bytes; the guided program and packet occupy 1,102 bytes. Manual discovery/review/write uses
three processes. Guide/freshness/review/write uses four and carries 9,027 additional internal
request/response bytes. This fixture measures protocol bytes and process counts; it runs no model
and establishes no token, quota or population result.

The existing inline-discovery task route supplies a stronger process baseline: its complete program
and packet occupy 1,217 bytes and use two processes. Guided delivery saves 115 exposed bytes while
adding two processes and 9,512 internal bytes. All three arms pass the same compiler, behavioral,
source, patch and lifecycle oracles. Counts use canonical JSON and retain every complete agent
program, environment-binding form, internal request, response and final packet size.

The freshness read and following preview use separate subprocesses. Execution still requires
review of the complete authoritative preview and its unchanged basis.

Workspace source-dependent recipes first return a bounded structure map, then ask for an exact
file or declaration handle for source disclosure. NUL-containing scalar values require complete
checked task delivery through JSON stdin; unchecked CLI scalar actions refuse them explicitly.

## Retaining the guide through delivery

`intent_action` names the versioned action schema, the operation kinds admitted for this route,
the SDK compile/execute methods and review/basis pointers. Unsupported routes offer no action kinds.
Read `skills/fr/references/intents.md` for operation inputs.

Author a matching `TaggedIntentAction` from `fr_ir.intent_actions`, then call
`client.compile_guided_intent(guide, action)`. Native compilation binds the retained goal and guide
basis to the same snapshot as the intent evidence. Inspect `/action/review` and execute the unchanged
compiled action with `client.execute_intent`. Every write uses declared checks and the existing
recoverable history/workflow engine. File/directory evidence provides bounded declaration continuations.

Capability goals can include a `range` with `start`/`end` byte offsets inside the selected source.
Extraction uses the full span; call inlining, rewrites and flow use its start. The guide derives
exact CLI positions from that range. Separate SDK action parameters from the range itself.

Formal scaffold actions initialize missing package files in their isolated planned snapshot and
review those files with the model. The agent authors properties and tactics; strict source checks
and Lake acceptance preserve explicit model claims and remaining obligations.
