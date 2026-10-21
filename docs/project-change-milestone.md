# Change a project and keep the answers trustworthy

This milestone checks whether `fr` carries a change through a project without trusting answers about old code.
The [two package milestones](package-milestones.md) and these two selected project-change outcomes now have passing original records.
The [combined record](../tests/agent-eval/results/2026-10-20-projects-project-changes/result.json) covers 21 change comparisons and both language deliveries.
The [current replay](../tests/agent-eval/results/2026-10-22-hash-lifetime-project-changes-recheck/result.json) repeats all of them with current answer hashing, memory ownership, release inputs and collection workflow.
For example, a saved finding about Boltons' `clamp` must agree with a fresh question after a rename or move.
The earlier replay records stay unchanged; this run makes no new unfamiliar-project claim.

Suppose a project uses a function named `subtotal`. The requested change renames it to `order_subtotal`.
An import makes that function available in another file; a caller invokes it.
Finding the declaration alone is insufficient. The change must include the required imports, callers and checks.
An answer saved before the rename may refer to a name or location that no longer exists.
The tool must discard affected findings while keeping unrelated findings whose inputs still match.

For an argument change, imagine requiring `format_total(amount, currency)` where callers previously supplied only `amount`.
Every required caller must supply its currency explicitly. The checks must verify the requested behavior.
These examples explain the task; they are not selected project results.

## Two outcomes checked together

| Outcome | What must work | Existing tracking requirement |
| --- | --- | --- |
| Trust answers after project changes | Saved and resumed answers agree with fresh checks after every declared change across three independent projects | `C.repository-mutations` |
| Finish a function change | One Rust task and one Python task include required callers, imports, configuration, behavior checks, undo and patch replay | `D.structural` |

A fresh check starts without the earlier saved answer and asks the same question about the same current files and explicit settings.
Agreement includes source locations, reported limitations and whether the question finished.
Two empty reports cannot establish agreement when one search never finished.

## Choose the tasks before adding support

The tasks are recorded in [the project selection](../tests/agent-eval/project-changes/tasks.json).
They were committed before adding language support or opening the reserved implementation.

| Project | Requested change | Check from a user's point of view |
| --- | --- | --- |
| Rust glob library | Rename `glob_with` to `glob_with_options` | The same files match the same patterns, with case sensitivity and other matching options supplied explicitly |
| Python Boltons library | Rename `clamp` to `clamp_value` | Import the original math module; limiting -5, 17 and 70 to the range 0 through 50 still returns 0, 17 and 50 |
| Python Humanize library | Keep discovery current through project changes | Find the implementation and uses of `intcomma`, shown in its public README as formatting 12345 as `12,345`; implementation and tests stayed unread until qualification |

All three projects have pinned source revisions. Glob and Boltons are development projects.
Humanize's implementation and tests stayed unread until the implementation, procedure and both checker builds were fixed.
Its first evaluation passed all seven change cases. The independent audit retained its original result and every failed or cancelled development attempt.
The catalog now links qualifying records for both outcomes.

Record each request, expected behavior, public interfaces in scope, required checks and reasons for selecting the project.
Use existing functions through their original project structure. A newly invented helper or copied package cannot qualify as a real project task.
Choose at least one project whose implementation and tests stay unread during development.
Public documentation and repository metadata may establish its question; record exactly what was read.
Boltons, dateutil, JDcal, Toolz, Funcy and Pydash are already familiar.

Fix the checker, explicit settings and change procedure before opening the unread implementation.
For the analyzer-change control below, record both builds before that opening too.
Run familiar-project checks first, then the first unread-project evaluation with those recorded inputs.
Keep its first outcome even if it fails. If inspecting its source guides a fix, it becomes a development project.
A later unfamiliar-project claim then needs another unread project.

## Apply the same changes across the three projects

| Change | Example | Required result |
| --- | --- | --- |
| Rename | Rename the selected helper and update its required uses | Old names and locations cannot authorize a new edit; current answers agree with fresh checks |
| Move | Move the helper to another file and update imports | Searches and saved findings reflect the new location |
| Delete | Remove a required helper or module | Report the missing input or unfinished question rather than reuse a complete answer |
| Add a competing import | Add a local file that changes which helper an import selects | Check the changed selection before trusting the earlier answer |
| Change project configuration | Change a project-declared check or source setting | Recheck findings and edit approvals that depend on the changed configuration |
| Change explicit settings | Change the user-supplied input/output rules or selected analysis settings | An answer checked under the old settings cannot satisfy the new request |
| Change the analyzer | Ask again using a second recorded build with an actual implementation change | Reject affected old results and compare against a fresh check with that second build |

Merely editing an analyzer identifier does not exercise the last case.
Record which changed implementation input should invalidate the answer.
The recorded control builds two checkers from real code: one accepts a new function
name up to 256 bytes; the other accepts up to 255 bytes. A 256-byte rename must
therefore pass one review and fail the other. Saved answers about the checker must
expire even though the project files and public data format stay the same.
For each change, also keep a finding about independent code. It must remain usable while its own recorded inputs still match.
Restore the pinned starting files between independent cases so one change cannot hide another.
Also test a saved task resumed in another process after an edit.

These comparisons change copies of each project outside `fr`. This checks whether
`fr` notices edits from another editor too. The delivery tasks below use `fr` to
review and apply the requested changes, then test the resulting programs.

## Deliver the Rust and Python changes

For each task, retain the starting behavior and a check that catches the requested problem or missing behavior.
Use the public discovery and edit operations to identify the declaration and required uses.
Explain every unresolved use. An incomplete search cannot establish that all callers were updated.
Include relevant imports, configuration and tests, then review and apply the proposed change.
Compile or execute the changed project and run its required behavior checks.
Undo the change and verify restoration of the starting behavior.
Export the patch, apply it in a separate checkout and rerun the checks there.
Save the original and changed sources, requests, commands, results and failures together.

Keep settings explicit. Add no compatibility wrappers, old calling forms or implicit argument values to make a task appear complete.
General language support may expand when the chosen task needs it. Check each new rule independently of that project's expected answer.
Keep existing refusals, source limits and execution deadlines. Fix the cause of a failure or timeout without extending them.

### Include the documentation that users read

Boltons' documentation contains `.. autofunction:: boltons.mathutils.clamp`.
This is a Sphinx instruction: the documentation builder imports that Python object
and uses its description. Renaming the function while leaving this instruction
unchanged would break the documentation even if the Python tests passed.

The procedure reviews the code rename through `fr`, then uses an exact
[`fr file replace`](file-transactions.md#exact-text-replacements) transaction for
that instruction. The required match count is one. Both transactions contribute
to the single delivered patch. A separate check imports all four objects named
in the original document. It must fail after the code rename, pass after the
documentation repair, and pass again after undo, redo and patch application.
This checks those references; it does not claim a complete Sphinx build.

### Use fr while developing fr

The hosted checks use the public Python client to read the actual functions
responsible for saved answers, import resolution, edits, file transactions and
discovery. Each request selects its source file explicitly. They also use public
discovery to list the original projects' declarations and consumers, save and
reopen findings, review the proposed changes, apply them, run checks and export
patches. Undo and redo use the same public history commands available to a user.

These runs have already exposed failures that unit examples missed. A Python
import such as `from helper import price as charge` must rename the imported
`price` while preserving calls to the local name `charge`. Two Rust functions
named alike in different modules must remain distinguishable when discovery
is saved. A one-file inspection must select that file before indexing; filtering
an entire source tree afterward caused a timeout. The fixes address these causes
and keep the original deadlines.

## Completion and continuing checks

Both outcomes need qualifying original results. The three projects must pass the full change procedure, including the first unread-project evaluation.
The Rust and Python tasks must pass their behavior, undo and separate-checkout delivery checks.
The catalog must link those results before either tracking requirement can close.
Passing unit tests or writing this plan does not finish the milestone.
The retained record now meets those criteria for the three selected projects and two rename tasks.
It preserves 32 failed or cancelled development attempts alongside the successful original results.

The recurring CI check replays the original result with its recorded source,
Rust 1.99.1 and Python 3.14.8. It also rejects altered completion records.
Later source changes need fresh results before tracking can call them checked.
The current replay repeats all 21 changes and both renames with the current
implementation, then independently checks the results. All three projects are
familiar during this replay. Its record links the unchanged first Humanize
evaluation; it makes no new unread-project claim. Tracking requires the fresh
replay, and CI checks both the original and current results.

The current replay keeps its raw files in a ZIP archive. This preserves original
line endings and project Git settings without applying those settings to this
repository. The full test suite also checks the current implementation.

Run builds, full tests and result collection on GitHub under the [workstation policy](development.md#working-on-a-shared-desktop).
This milestone checks correctness through changes. Measure speed and storage separately against fresh checks doing the same work, including setup costs.
The [saved-work comparison](saved-work-cost-results.md) records those costs on glob, Boltons and JMESPath.
The [matched SDK follow-up](saved-answer-improvement.md) checks the hashing fix with the same native executable and complete session costs.
It does not establish lower agent costs, broad package support, numeric translation or source-connected proofs.
See the [plan](../PLAN.md) and [current tracking](roadmap-status.md) for those larger goals.
