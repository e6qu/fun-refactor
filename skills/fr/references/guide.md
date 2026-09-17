# Start with a structured goal

Run `fr guide --from GOAL` or `FrClient.guide(AgentGoal(...))`. Supply a purpose, exact `target` or
one-name/path `selector`, and tagged `operation`; `automatic` selects the purpose's route. Read
`state`, refusals, uncertainty and `author_fields`. Never select ambiguous candidates by row order.

```sh
fr guide --from '<GOAL>'
```

Use ready `actions[].arguments` and optional stdin `input`. A non-ready action names what to author;
its `reference` links the specialized contract. Guides recommend previews only. Review the complete
basis before execution.

Python imports `AgentGoal`, `GoalSelector`, `GoalOperation`, `GoalConstraints` and `GoalLimits` from
`fr_ir.guide`. Use `client.complete_guide` only to follow read and preview actions. For writes,
author one typed `TaggedIntentAction`, call `review = client.review_guide(guide, action)`, inspect
the complete review, then call `client.execute_guide(review)`. The executor refreshes the guide and
accepts only its unchanged native review. Keep reports local and expose selected fields with `at`.
For a complete semantic scalar goal, pass `guide.semantic_scalar_action()` to `review_guide`; it
returns the exact task operation committed by the guide.

For non-ready actions, pass `GuideInputs({"field-name": value})`; wrap recipe, property, plan or
tactics text in `GuideFile("plain-name", text)`. `client.complete_guide(goal, {action_index:
inputs})` follows all read/preview actions inside one local program. It refreshes the guide basis for
every action, requires the exact named fields, verifies output schemas and removes temporary files.
It never writes project source. The agent must still author and review every supplied value.

Allow source through `constraints.allow_source` only when the route requires an explicit bounded
reveal. Keep the six verification levels separate. An agent writes every property and tactic;
generated Rust model theorems do not establish implementation correspondence. Full wire contracts:
`docs/agent-workflow-guide.md`.

Read `intent_action` for admitted operation kinds and review/basis pointers. For delivery, use
`client.review_guide(guide, TaggedIntentAction(OPERATION))`. Read [Intents](intents.md) for the
operation mirrors. Native
compilation revalidates the original goal and basis in the same snapshot as its evidence; do not
reconstruct a different target from a name or a position.
