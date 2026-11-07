# Why the page-count diagnostic failed


The prediction failed: neither agent delivered a complete accepted result in its two attempts.
All four reached the unchanged 150-second limit without a patch.
[Independent checking](https://github.com/e6qu/fun-refactor/actions/runs/41289141428) retained all four failures.
No service error, collection failure, changed checking input or resource stop other than the deadline appeared in these records.

| Attempt | First returned proof-check response | Observed obstacle |
| --- | ---: | --- |
| GLM 1 | 33.157 seconds | Admitted steps were refused; later drafts had Lean errors. Some reports were cut off with `head`. |
| Kimi 1 | No draft check | Read the claim and searched division lemmas until the deadline. |
| GLM 2 | 56.614 seconds | Fr rejected nested assumption patterns; later drafts reached Lean but still failed. |
| Kimi 2 | 110.385 seconds | Late checking exposed unresolved goals and a deprecated library name. One report was cut off with `tail`. |

The first-response times include refusals before Lean runs; they do not mean a proof passed.
No accepted proof write or passing package check was observed.
Unavailable stage times remain unknown.

For example, `⟨⟨a, b⟩, c⟩` unpacks grouped assumptions.
A separate minimal program confirms that Lean accepts these nested patterns while the measured fr rejects them.
That establishes a reader defect. It does not establish that the complete submitted drafts would otherwise pass.
A [direct Lean check of the exact affected draft](../direct-lean-control-summary.json) confirms independent proof errors.
The draft tries to introduce an assumption that is already present and uses an unavailable tactic. Lean also reports an unfinished final example.
Later manual checks show that some messages disappear when earlier errors change; each message need not represent an independent mistake.
The recursive-pattern repair comes after these frozen attempts and cannot reclassify them.

The repaired reader passed all 29 hosted preparation checks.
An [exact replay](../reader-control-summary.json) now reaches Lean and still rejects the original draft.
The [manual error review](../draft-error-review.json) separates assumption grouping, Boolean conditions and missing division facts.
Its final manual control proves the unchanged complete claim; it supplies no new agent result or timing comparison.
The full comparison stays paused. Freeze a small diagnostic about finding and using the missing facts before another paid study.
Keep the manually completed proof out of agent inputs.
These four attempts have no ordinary-tool baseline, so they cannot measure fr overhead or establish a causal improvement.
The 335 earlier M3 outcomes and twelve earlier diagnostic outcomes remain unchanged.
