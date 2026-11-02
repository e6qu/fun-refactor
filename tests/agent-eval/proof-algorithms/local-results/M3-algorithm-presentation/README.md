# Can guidance help the agents prove an executable algorithm?

The same executable Euclid algorithm, fr, shared skills, declarations, plan nodes and dependencies. Compare checklist and graph rendering only; no automatic search controller.

The fixed Lean function must terminate and return the correct GCD for every pair of Nat inputs.
Each complete attempt has 150 seconds including tools, checks, patch delivery and session accounting.
These small development comparisons do not close M3 or measure fr overhead against ordinary tools.
Two repetitions cannot establish a general success rate. Keep every failure in the total work.

| Agent | checklist: accepted / 2 | graph: accepted / 2 | Total seconds, including failures |
| --- | ---: | ---: | ---: |
| codex-luna-fast | 2 | 2 | 312.805 |
| opencode2-glm-flash | 0 | 0 | 600.212 |
| opencode2-kimi-k3 | 1 | 2 | 438.050 |

codex-luna-fast: No observed success advantage; compare total work including failures without claiming equal methods in general.
opencode2-glm-flash: The comparison cannot isolate guidance when service failures, collection failures or system sleep disrupt sessions. Retain every original outcome, diagnose the disruption and report the comparison as inconclusive; do not replace failed slots or claim that guidance caused the difference.
opencode2-kimi-k3: Inconclusive with two repetitions. Report the actions and failures; do not rerun slots until success.

Guidance size in UTF-8 bytes (not model tokens):

| Condition | Declarations | Plan |
| --- | ---: | ---: |
| checklist | 382 | 1131 |
| graph | 382 | 1155 |

Only reviewed summaries and submitted patches are public. Raw local records remain private.

| Attempt | Complete outcome | Seconds | Failure or missing result | Patch |
| --- | --- | ---: | --- | --- |
| codex-luna-fast-euclid-checklist-1 | passed | 60.491 |  | [patch](patches/codex-luna-fast-euclid-checklist-1.patch) |
| codex-luna-fast-euclid-graph-1 | passed | 74.830 |  | [patch](patches/codex-luna-fast-euclid-graph-1.patch) |
| opencode2-glm-flash-euclid-graph-1 | failed | 150.056 | Misread an already-present termination proof, then revised failing correctness drafts. Timed out without a patch. | none |
| opencode2-glm-flash-euclid-checklist-1 | failed | 150.050 | A fr reader rejection blocked a multiline proof form accepted by Lean in a separate control. The original draft also had Lean errors. No patch. | none |
| opencode2-kimi-k3-euclid-checklist-1 | passed | 91.294 |  | [patch](patches/opencode2-kimi-k3-euclid-checklist-1.patch) |
| opencode2-kimi-k3-euclid-graph-1 | passed | 113.852 |  | [patch](patches/opencode2-kimi-k3-euclid-graph-1.patch) |
| codex-luna-fast-euclid-graph-2 | passed | 92.720 |  | [patch](patches/codex-luna-fast-euclid-graph-2.patch) |
| codex-luna-fast-euclid-checklist-2 | passed | 84.763 |  | [patch](patches/codex-luna-fast-euclid-checklist-2.patch) |
| opencode2-glm-flash-euclid-checklist-2 | failed | 150.058 | Directly edited both proofs, checked an empty termination draft and changed induction inputs repeatedly. Timed out without a patch. | none |
| opencode2-glm-flash-euclid-graph-2 | failed | 150.047 | Repeated an unsupported timeout option and directly edited both proofs. The supplied draft was still empty until late in the session. No patch. | none |
| opencode2-kimi-k3-euclid-graph-2 | passed | 82.894 |  | [patch](patches/opencode2-kimi-k3-euclid-graph-2.patch) |
| opencode2-kimi-k3-euclid-checklist-2 | failed | 150.010 | Passed proof and package checks, then wrote the diff to the parent directory instead of the requested project-root file. Complete attempt timed out. | none |
