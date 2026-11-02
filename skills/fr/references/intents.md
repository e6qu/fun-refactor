# Reviewed intents

Request a change, such as a new retry limit. Review its preview before execution.

An intent names the task and action; a packet holds fields. Its basis binds execution to the reviewed preview.

Import from `fr_ir.intent` and `fr_ir.intent_actions`. Compile a packet, inspect `/action/review`, then execute its unchanged identity.

| Operation | Purpose | Input |
|---|---|---|
| `TaskChangeOperation` | change | Existing `TaskChange`; bounded requests and multiple targets |
| `AuthorBatchOperation` | change | Existing author batch manifest, checks, delivery |
| `RecipeOperation` | change | Agent-authored inline DSL, checks, delivery |
| `CapabilityOperation` | any for reads; change for writes | Capability, scalar parameters; optional byte range; writing checks/delivery |
| `FrameworkMigrationOperation` | migrate | Feature, destination framework/output, checks, delivery; optional connection/cutover fields |
| `ProjectQueryOperation` | any | Bounded `ProjectRequest` values including the exact target |
| `SurfaceEditOperation` | change | Returned surface edit ID, replacement, checks, delivery |
| `PropertyTaskOperation` | prove | Exact source declaration |
| `FormalPlanOperation` | prove | Property kinds and typed agent properties; optional scaffold package/checks/delivery |
| `ProofTaskOperation` | prove | Exact obligation |
| `ProofSubmissionOperation` | prove | Exact obligation, agent-written tactics, checks, delivery |

```python
from fr_ir.intent import AgentIntent, IntentNeed
from fr_ir.intent_actions import TaggedIntentAction, TaskChangeOperation

compiled = client.compile(AgentIntent(
    handle, "change", needs=(IntentNeed("map", "code_map", "/target"),),
    packet_limit=65536, action=TaggedIntentAction(TaskChangeOperation(change)),
))
review = compiled.at("/action/review")
result = client.execute_intent(compiled)
```

From a navigator, use `client.review_guide(guide, action)`. Fr rechecks the original goal and basis in the evidence snapshot.
Operation, checks, proof expectation and delivery must match that goal. Keep the review.
Changes to source, input, evidence or package settings invalidate its `fraa2:` identity.

`additional_evidence` binds targets and declaration locations to one snapshot's hashes.
File and directory maps have bounded continuations. Select only needed fields;
raise the packet ceiling only for a complete review.

Capabilities keep their scope; restructuring, entry points and stitching can cover the workspace.
Declaration recipes require an exact target; ambiguous names need task/capability handles.
Byte spans stay in selected source. Calls and rewrites use their start.

The agent authors every property and tactic. `FormalPlanOperation(..., package=..., checks=...,
delivery=...)` stages a scaffold; without those writing fields it is read-only. Scaffolds and proofs
pass strict source/signature checks and a Lake build in an isolated planned package before history.
Named scaffold debt stays explicit. Submit tactics without leading `by`; Lean checks the exact
goal before recording a receipt. Set `proof_expectation="model"` only
for a checked submitted theorem. Implementation expectations refuse without separate evidence.
Inspect `claims`, proof receipt, package validation and remaining obligations; a model theorem does
not establish general implementation correspondence.

Read-only packets cannot execute.
