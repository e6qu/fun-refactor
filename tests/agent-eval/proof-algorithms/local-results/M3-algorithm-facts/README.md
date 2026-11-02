# Can guidance help the agents prove an executable algorithm?

The same executable Euclid algorithm, fr and shared skills. Compare selecting library facts with supplied declarations.

The fixed Lean function must terminate and return the correct GCD for every pair of Nat inputs.
Each complete attempt has 150 seconds including tools, checks, patch delivery and session accounting.
These small development comparisons do not close M3 or measure fr overhead against ordinary tools.
Two repetitions cannot establish a general success rate. Keep every failure in the total work.

| Agent | select-facts: accepted / 2 | supplied-declarations: accepted / 2 | Total seconds, including failures |
| --- | ---: | ---: | ---: |
| opencode2-glm-flash | 0 | 0 | 600.161 |
| opencode2-kimi-k3 | 0 | 1 | 594.378 |

opencode2-glm-flash: Both conditions failed. Inspect actual tactics, checking and delivery before proposing another experiment.
opencode2-kimi-k3: Inconclusive with two repetitions. Report the actions and failures; do not rerun slots until success.

Guidance size in UTF-8 bytes (not model tokens):

| Condition | Declarations | Plan |
| --- | ---: | ---: |
| select-facts | 71 | 83 |
| supplied-declarations | 382 | 83 |

Only reviewed summaries and submitted patches are public. Raw local records remain private.

| Attempt | Complete outcome | Seconds | Failure or missing result | Patch |
| --- | --- | ---: | --- | --- |
| opencode2-glm-flash-euclid-select-facts-1 | failed | 150.066 | Saved termination; correctness drafts failed. Looked up the GCD recursion rule near the deadline and timed out without a patch. | none |
| opencode2-glm-flash-euclid-supplied-declarations-1 | failed | 150.024 | Edited both proofs directly. Errors in correctness blocked later termination checks; unsupported command options also consumed time. No patch. | none |
| opencode2-kimi-k3-euclid-supplied-declarations-1 | passed | 144.283 |  | [patch](patches/opencode2-kimi-k3-euclid-supplied-declarations-1.patch) |
| opencode2-kimi-k3-euclid-select-facts-1 | failed | 150.023 | Correctness drafts used ordinary induction and an unavailable tactic. Output filtering hid some write confirmation. Timed out without a patch. | none |
| opencode2-glm-flash-euclid-supplied-declarations-2 | failed | 150.040 | Edited both proofs directly and repeatedly checked an empty draft. Induction inputs still disagreed; the package failed and no patch arrived. | none |
| opencode2-glm-flash-euclid-select-facts-2 | failed | 150.030 | Repeated unsupported command options, then changed termination while errors in another proof still blocked checking. Timed out without a patch. | none |
| opencode2-kimi-k3-euclid-select-facts-2 | failed | 150.069 | Used an unavailable induction-rule name and invalid branch syntax while repairing correctness. Timed out without a patch. | none |
| opencode2-kimi-k3-euclid-supplied-declarations-2 | failed | 150.004 | Passed both draft checks but spent time interpreting the repeated write result. Correctness was not saved and no patch arrived before timeout. | none |
