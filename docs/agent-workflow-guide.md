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
select semantic deltas; migration requires an advertised destination; proving selects the
formalization workbench for admitted targets or a Lean proof task.
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
| `source-body` | Complete body in the target language, after `allow_source: true` | Bounded source reveal and `replace-body` writer |
| `source-bodies` | Complete bodies for one primary and up to seven additional exact declarations | Bounded reveals and one reviewed multi-target body change |
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
An exact declaration target carries the same `location.name` and `location.definition` byte spans
and line ranges through its guide, disclosure and native intent packets.
Recipe guidance includes only its live verb, relevant selector fields, target values, support and
matched/refusal expectation forms. Target-kind and language incompatibility refuses before authoring.
`source-body` requires `constraints.allow_source: true`. Its first action reveals only the selected
declaration under the goal's token limit. The next action previews a complete body fragment through
the existing source writer. A typed `TaskChange` binds that body to the reviewed guide, checks and
delivery stages. Use this route when the semantic body has no exact IR identity but the source
writer admits the target language.
`source-bodies` uses the primary goal selector plus `operation.additional` selectors. Each must
resolve to a distinct declaration admitted by `replace-body`. The guide reveals every target
under the same source limit and previews one `author batch` manifest. Native review checks that
the authored target set equals the guide's complete target set before one checked delivery.

In the Python SDK, `guide.source_body_action({handle: body_text})` builds that task from the
retained targets, named checks and delivery policy. Supply every guided handle exactly once;
names and list positions are not substitutes for handles. Python bodies are relative suites;
braced languages include the outer braces. The helper copies the supplied text, derives exact
file/edit postconditions and makes no subprocess call. For example, after reading a single
Python target and authoring its replacement:

```python
action = guide.source_body_action({guide.target.handle: "return value.strip().upper()\n"})
review = client.review_guide(guide, action)
print(review.at("/diff"))  # Inspect the complete review before executing it.
```

Execute an accepted review with `client.execute_guide(review)`. A prepared action is not an
approved write: native review still rejects stale source, invalid bodies and incomplete diffs.
The helper refuses changed guide/goal receipts, incomplete target sets and unadmitted routes.
It does not infer replacement bodies or choose among ambiguous declarations.

If a declared check fails after application, the transaction can remain applied and no patch is
delivered. Keep the failed execution report (including `FrRuntimeError.report` on a nonzero exit),
inspect its `transaction` and `workflow.stages`, then reopen the project and preview
`fr history undo TX`. After reviewing that reversal, use `fr history undo TX --write --no-diff`
and rerun the declared checks. Undo refuses if later edits conflict; preserve those edits and
resolve them deliberately. Do not report a failed checked edit as a delivered change.

Exact scalar goals with declared checks produce one complete `fr-task-change-1` preview input.
Integer/float scalars follow the live unsigned-decimal contract; negatives use explicit unary IR
nodes. Option-like string values use equals-form arguments so the CLI preserves their data role.
The native task planner checks uniqueness, scalar category, writer admission and postconditions
before the guide advertises it. The common Python delivery API wraps the corresponding typed task
operation in the same `GuideReview` used by every other writable route.
`guide.semantic_scalar_action()` returns that typed operation only when the ready task manifest
exactly matches the retained goal, target, checks, delivery and postconditions.

```python
from fr_ir.guide import AgentGoal, GoalOperation, GoalSelector
from fr_ir.intent_actions import CapabilityOperation, TaggedIntentAction
from fr_ir.ir import TaskDelivery
from fr_ir.runtime import FrClient

client = FrClient(".")
delivery = TaskDelivery(patch="artifacts/change.patch", check_output_bytes=256)
goal = AgentGoal(
    "change",
    selector=GoalSelector(name="calculate", scope="src", language="rust"),
    operation=GoalOperation("capability", {
        "capability": "rename", "parameters": {"new_name": "evaluate"},
    }),
    checks=("unit",),
    delivery=delivery,
)
guide = client.guide(goal)
review = client.review_guide(guide, TaggedIntentAction(CapabilityOperation(
    "rename", {"new_name": "evaluate"}, checks=("unit",), delivery=delivery,
)))
print(review.at("/diff"))
result = client.execute_guide(review)
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
still authors every required value and reviews the returned reports; a `GuideRun` is never an
execution request. For writes, `review_guide` accepts the retained guide and exactly one typed
operation admitted by its route. Its `GuideReview` binds the normalized goal, guide basis, authored
input digest, target set, revision, checks, delivery policy and complete native preview.
`execute_guide` checks the retained goal and review for mutation, then submits the exact reviewed
intent. Native execution recomputes the guide in the edit snapshot before recording history.
There is no separate SDK guide read before writing. Read/preview actions still refresh their guide.

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

The [complete-program comparison](../tests/agent-eval/agent-guide-context.json) changes a generic
Rust scalar, runs compiler and finite behavioral checks, emits an identical patch and completes
all eight lifecycle stages. It compares guided delivery with separate discovery/task review and
with the stronger inline-discovery task baseline. Counts retain complete prescribed programs and
canonical request/response sizes. They establish neither autonomous agent efficiency nor billing.

The [workflow inventory and comparison](workflow-routes.md) explains which routes add distinct
checks, where their instructions overlap, and the broader before/after consolidation measurement.
Execution always requires the complete authoritative preview and its unchanged basis.

Workspace source-dependent recipes first return a bounded structure map, then ask for an exact
file or declaration handle for source disclosure. NUL-containing scalar values require complete
checked task delivery through JSON stdin; unchecked CLI scalar actions refuse them explicitly.

## Retaining the guide through delivery

`intent_action` names the versioned action schema, the operation kinds admitted for this route,
whether it is writable, the SDK review/execute methods and review/basis pointers. Writable routes
also carry `fr-guide-delivery-1`, which states the common lifecycle and every required identity.
Unsupported routes offer no action kinds.
Read `skills/fr/references/intents.md` for operation inputs.

Author a matching `TaggedIntentAction` from `fr_ir.intent_actions`, then call
`client.review_guide(guide, action)`. Native compilation binds the retained goal and guide basis to
the same snapshot as the intent evidence. Inspect the returned review and execute it unchanged with
`client.execute_guide`. Every write uses declared checks and the existing
recoverable history/workflow engine. File/directory evidence provides bounded declaration continuations.

Capability goals can include a `range` with `start`/`end` byte offsets inside the selected source.
Extraction uses the full span; call inlining, rewrites and flow use its start. The guide derives
exact CLI positions from that range. Separate SDK action parameters from the range itself.

Formal scaffold actions initialize missing package files in their isolated planned snapshot and
review those files with the model. The agent authors properties and tactics; strict source checks
and Lake acceptance preserve explicit model claims and remaining obligations.
