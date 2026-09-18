# Reviewed intents

Import intents from `fr_ir.intent`, operations from `fr_ir.intent_actions`. Compile a bounded
packet, inspect `/action/review`, then execute its unchanged identity.

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

When starting from a navigator, use `client.review_guide(guide, action)`; native `fr`
revalidates the original goal and guide basis inside the evidence snapshot. Authored operation,
checks, proof expectation and delivery must agree with that goal. Retain the returned review;
changed source, input, evidence or package configuration invalidates its `fraa2:` identity.

Task references and `additional_evidence` bind each target in one snapshot. The SDK stores
selected Merkle roots. File/directory maps provide bounded declaration continuations. Choose
small projections; increase the packet ceiling only for a complete review.

Capabilities preserve original scope; restructuring, entry points and stitching can cover the
workspace. Declaration recipes must resolve exactly the target; ambiguous paths/names require
task/capability handles. Byte spans stay inside the selected source. Calls and rewrites use the start.

The agent authors every property and tactic. `FormalPlanOperation(..., package=..., checks=...,
delivery=...)` stages a scaffold; without those writing fields it is read-only. Scaffolds and proofs
pass strict source/signature checks and a Lake build in an isolated planned package before history.
Named scaffold debt stays explicit. Submit tactics without leading `by`; Lean checks the exact
goal before recording a receipt. Set `proof_expectation="model"` only
for a checked submitted theorem. Implementation expectations refuse without separate evidence.
Inspect `claims`, proof receipt, package validation and remaining obligations; a model theorem does
not establish general implementation correspondence.

Legacy `IntentAction(change)` supports one direct target. Read-only packets cannot execute.
