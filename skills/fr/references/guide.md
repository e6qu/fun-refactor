# Start with a structured goal

Run `fr guide --from GOAL` or `FrClient.guide(AgentGoal(...))`. Use purpose `understand`, `trace`,
`change`, `migrate` or `prove`, a full `target` handle or `selector` with one `name`/`path`, and a tagged
`operation`. Default `automatic` selects the current purpose's route. Read `state`, `refusals`,
coverage, uncertainty and named `author_fields`; never resolve ambiguous candidates by row order.

```sh
fr guide --from '<GOAL>'
```

Use exact ready `actions[].arguments` and optional stdin `input`. A non-ready action names the only
content you must author. Load the returned `reference` from this directory when that content needs
its specialized contract. Guides recommend previews only. Review the complete authoritative basis
before using the existing execution method.

Python imports `AgentGoal`, `GoalSelector`, `GoalOperation`, `GoalConstraints` and `GoalLimits` from
`fr_ir.guide`. `GoalOperation("semantic-scalar", {"operation":"set-int","from":"7","to":"9"})`
with declared checks emits one complete task preview. `client.follow_guide` verifies fresh guidance
and returns `TaskReview`; review it, then use `client.execute`. Retain intermediate reports locally
and expose only selected fields with `at`.

Allow source through `constraints.allow_source` only when the route requires an explicit bounded
reveal. Keep the six verification levels separate. An agent writes every property and tactic;
generated Rust model theorems do not establish implementation correspondence. Full wire contracts:
`docs/agent-workflow-guide.md`.

Read `intent_action` for admitted operation kinds and review/basis pointers. For delivery, use `client.compile_guided_intent(guide,
TaggedIntentAction(OPERATION))`. Read [Intents](intents.md) for the operation mirrors. Native
compilation revalidates the original goal and basis in the same snapshot as its evidence; do not
reconstruct a different target from a name or a position.
