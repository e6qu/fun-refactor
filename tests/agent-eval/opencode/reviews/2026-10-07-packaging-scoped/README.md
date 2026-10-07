# Three scoped packaging reviews, two models each

This collection assigns eleven requirement areas to three focused questions:
fallback and bounds; explicit policies; objects, iteration and unchanged APIs.
Both configured models receive each question in frozen order, at `low` reasoning
effort and the existing 2,048-token output allowance. The six cells are separate
review attempts, not retries of earlier failures.

`prepare.py` reconstructs the design from the unchanged whole-task source archive.
Every question supplies the complete requirement and grader, plus selected function
signatures and executable bodies. Docstrings and other source remain available
through read-only tools. Prompts are 11,164–12,580 bytes; the narrower questions do
not establish lower total context cost. They request one concise structured result
without a prose preface, and explicit limitations for their assigned areas.

The plan must be committed before calls. It binds the question preparation,
coverage implementation, catalog and prior hosted/workstation admission evidence.
Both executables must match the admitted workstation control. Capture limits remain
120 seconds, 20 CPU seconds, 768 MiB sampled aggregate RSS, 16 MiB disk growth and
1 MiB transcript. Local calls additionally use the user's resource guard.

The collection stops permanently at its first resource-limit failure, at two
consecutive failed reviews, or after all six attempts. Old collections remain
stopped. No candidate code runs during review.

`tools/review-coverage.py` replays the collection and lists assignments, each
model's submission status, findings and limitations. It never marks the task
accepted, even when every submission is complete and all finding lists are empty.
Acceptance requires inspecting the limitations and independently checking claims.

All six cells ran once after commit `8b42548e`. Five submitted valid answers; GLM's
policy review exhausted its output allowance with 2,046 reasoning tokens and two
other output tokens. No attempt hit a resource limit. The collection is closed.

| Question | Kimi K3 | GLM 5.3 Flash |
| --- | --- | --- |
| Fallback and bounds | Completed, 24.00 s, 536.52 MiB | Completed, 35.96 s, 552.95 MiB |
| Explicit policies | Completed, 62.80 s, 575.55 MiB | Failed, 56.11 s, 552.62 MiB |
| Objects and APIs | Completed, 38.16 s, 562.00 MiB | Completed, 45.82 s, 441.75 MiB |

Seven of eleven assigned areas have submissions from both models. Four policy
areas have only Kimi submissions. Empty finding lists do not establish acceptance.
Kimi's object/API answer repeated the earlier incorrect postrelease claim outside
its assigned scope. `assessment.json` rejects that finding against the unchanged,
previously verified snapshot and preserves both the answer and the policy failure.

Kimi used fr once during the policy review. An exact `Specifier.contains` lookup
returned no names, then ordinary search and reads supplied the additional context.
The CLI matches bare declaration names; the public exploration guide now makes
that contract explicit. This call demonstrates attempted use, with no useful fr
source delivery or measured efficiency benefit.

`verify-policy.py` independently checks constructor/setter/call combinations on
GitHub using the existing pinned grading image and limits. It also verifies that
only `SpecifierSet.filter` changed and all source outside that method remains
byte-identical. This check addresses recorded policy and unchanged-API limitations;
its result is separate from model submission status and task acceptance.

GitHub run [37609277041](https://github.com/e6qu/fun-refactor/actions/runs/37609277041)
passed all 324 policy combinations. `policy-verification/` retains the grader, raw
result and run provenance. The downloaded archive matched GitHub's artifact digest.
`tools/test-packaging-admission.py` checks the program, source, grader and result
identities offline without executing candidate code.

`admission.json` records a later explicit assessment of all eleven requirements.
It accepts this task for one bounded four-attempt pilot, with adapter controls and
frozen inputs still required before calls. The original `assessment.json`, failed
GLM review and automatic coverage report remain unchanged. This decision does not
establish exhaustive grader coverage or an efficiency advantage.

All usage figures are client counters. Subscription metadata reports zero cost;
actual billing and complete context accounting remain unknown.
