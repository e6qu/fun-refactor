# What the supplied-proof comparison tells us

All twelve fixed slots ran once. Each received two correct proof drafts for the
same executable GCD program. One instruction setup also included a short finish
card. The task was to check and save both proofs, check the package, export both
saved edits and finish the native session within 150 seconds.

[Independent GitHub checking](https://github.com/e6qu/fun-refactor/actions/runs/41645916940)
accepted all eleven delivered patches. They contain the same two supplied proofs;
the algorithm and claims are unchanged. Five attempts meet every completion
requirement. Six fail complete accounting and one times out without a patch.
The [generated report](README.md), [summary](summary.json) and
[independent verdicts](independent-checks.json) retain all twelve original outcomes.

| Agent | Correct delivered patches | Complete: shared skill | Complete: finish card |
| --- | ---: | ---: | ---: |
| Luna Fast | 4 / 4 | 2 / 2 | 2 / 2 |
| GLM Flash | 4 / 4 | 1 / 2 | 0 / 2 |
| Kimi K3 | 3 / 4 | 0 / 2 | 0 / 2 |

## Why correct patches do not make eleven complete successes

OpenCode's JSON stream records token counts at the end of each assistant response.
Its saved session provides a second record of those responses. In six completed
sessions, the saved final response has a normal stop reason and token counts,
but the stream has no corresponding final usage event. All earlier recorded
usage agrees. The missing record is the final reply, not a failed Lean command.
The [aggregate accounting review](accounting-review.json) records the counts and
hashes without publishing the raw session or response text.

The fixed rule requires complete, matching stream and session accounting. We
therefore leave complete usage unknown and those six attempts failed. We do not
fill the gap from another source or rerun them. The single GLM shared-skill session
with complete accounting remains a complete success. Its two saves report that fr
checked the proofs internally; it did not invoke separate `proof-check` commands.

The exporter already caught the accounting mismatch. The runner had initially
reported a completed native session because it checked the saved outcome without
requiring a usage event for every response. The repair requires exact response
coverage and rejects duplicates as well as omissions. Local execution marks
accounting incomplete immediately. Native admission requires the same condition.
A regression supplies a successful saved session with a missing final stream
event, then two copies of the earlier event: both must fail and preserve the export.
This repairs failure detection; it does not claim to repair OpenCode's event emission.

## Why the remaining Kimi attempt timed out

Kimi's first shared-skill attempt passed both proof checks, saved both proofs and
passed the full package check. It then tried `fr history --limit 4` twice. That
option does not exist. Fr returned an explicit usage error, and the session reached
150 seconds without a root `change.patch`. The native observer recorded no
collection or monitoring error. Both the timeout and missing patch remain failures.

This is a delivery-navigation failure after accepted proofs. It supplies no evidence
that Lean needed more time. Keep the deadline and use the existing saved-edit
export command, for example `fr history patch 1 2 --output change.patch`.

## What happens next

Luna's two setups tie. The GLM/Kimi accounting failures prevent a reliable
comparison of their instructions. No profile meets the fixed two-versus-zero rule
for testing the card on a different delivery task. Do not adopt the card as a
measured improvement, add graph search, or reopen these slots.

Before a new OpenCode comparison, reproduce the missing final event with no model
access, resolve it in an explicitly selected client, and qualify both tool replies
and final text replies against saved session accounting. A scripted control that
passes once cannot establish reliability of live-provider event completion.
Then return to the original page-count proof obstacle with a specific prediction.

These are supplied proofs, not proof discovery. There is no ordinary-Lean baseline
and no measurement of fr overhead. Exact first patch-creation times and actual
charges remain unknown. The measured complete-attempt time bounds patch capture;
it must not be presented as the exact creation time. M3 remains open.

The original twenty algorithm outcomes, the misplaced-patch diagnostic, all earlier
M3 comparisons and reserved list-reversal task remain unchanged. Raw prompts,
transcripts, session exports and source archives remain private. Only reviewed
summaries and submitted patches are public.
