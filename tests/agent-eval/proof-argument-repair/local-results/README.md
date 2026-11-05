# Eight complete page-count proof attempts

[Independent GitHub grading](https://github.com/e6qu/fun-refactor/actions/runs/41918858328) checked every outcome. All eight failed at delivery; none submitted a patch.

| Agent | Instruction | Repetition | First whole-proof check | Passing draft | Complete seconds |
| --- | --- | ---: | ---: | --- | ---: |
| GLM Flash | Shared skill | 1 | 91.11 s | no | 140.394 |
| GLM Flash | Repair instruction | 1 | 138.82 s | no | 140.405 |
| Kimi K3 | Repair instruction | 1 | 86.78 s | yes | 140.393 |
| Kimi K3 | Shared skill | 1 | 105.17 s | no | 140.502 |
| GLM Flash | Repair instruction | 2 | 87.59 s | no | 140.457 |
| GLM Flash | Shared skill | 2 | 67.12 s | no | 140.477 |
| Kimi K3 | Shared skill | 2 | unobserved | unobserved | 140.496 |
| Kimi K3 | Repair instruction | 2 | unobserved | unobserved | 140.412 |

A passing draft is not a saved proof or submitted patch. Kimi reached that stage once, near 135 seconds. No mapped argument-type error was observed, so the repair mechanism remains unobserved in all eight attempts. Some diagnostics were shortened; absence from the record does not prove an error never occurred.

The full attempts total 1,123.536 seconds. The [earlier interrupted collection](../interrupted-collection.json) adds 150.066 seconds and seven permanently unstarted slots. It is separate from this comparison and must remain in the overall cost account.

Every model process reached its allotted deadline. The ten-second finishing reserve kept each complete attempt inside the original 150-second limit. Session exports and observed tokens remain available privately; interrupted response usage and account charges remain unknown.

[Summary](summary.json) contains reviewed observations, settings, resource measurements and independent verdicts. [Checking receipt](checked.json) records the independent outcomes. No raw transcripts, prompts, drafts, archives, account data or unsubmitted proofs are published.

The baseline is the same agent with the shared skill. This does not measure fr overhead against ordinary Lean tools, establish a general success rate or close M3. See the [comparison and next work](../README.md).
