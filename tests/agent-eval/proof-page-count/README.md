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
A [direct Lean check of the exact affected draft](direct-lean-control-summary.json) confirms independent proof errors.
The draft tries to introduce an assumption that is already present, uses an unavailable tactic and leaves the final example unfinished.
Fixing the reader should expose those errors through fr; it cannot count as a successful proof.
The recursive-pattern repair comes after these frozen attempts and cannot reclassify them.

The full comparison stays paused. Fix and check the reader, then review the first unresolved proof or delivery step before another paid study.
These four attempts have no ordinary-tool baseline, so they cannot measure fr overhead or establish a causal improvement.
The 335 earlier M3 outcomes and twelve earlier diagnostic outcomes remain unchanged.
