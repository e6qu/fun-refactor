# Change a project and keep the answers trustworthy

The next milestone asks whether `fr` helps carry a change through a project without trusting answers about old code.
The [two package milestones](package-milestones.md) are complete. This milestone is open.

Suppose a project uses a function named `subtotal`. The requested change renames it to `order_subtotal`.
An import makes that function available in another file; a caller invokes it.
Finding the declaration alone is insufficient. The change must include the required imports, callers and checks.
An answer saved before the rename may refer to a name or location that no longer exists.
The tool must discard affected findings while keeping unrelated findings whose inputs still match.

For an argument change, imagine requiring `format_total(amount, currency)` where callers previously supplied only `amount`.
Every required caller must supply its currency explicitly. The checks must verify the requested behavior.
These examples explain the task; they are not selected project results.

## Two outcomes to complete together

| Outcome | What must work | Existing tracking requirement |
| --- | --- | --- |
| Trust answers after project changes | Saved and resumed answers agree with fresh checks after every declared change across three independent projects | `C.repository-mutations` |
| Finish a function change | One Rust task and one Python task include required callers, imports, configuration, behavior checks, undo and patch replay | `D.structural` |

A fresh check starts without the earlier saved answer and asks the same question about the same current files and explicit settings.
Agreement includes source locations, reported limitations and whether the question finished.
Two empty reports cannot establish agreement when one search never finished.

## Choose the tasks before adding support

No projects or tasks have been reserved for this milestone yet. Selection is the first work item.
Choose three independently maintained projects with pinned source revisions and concrete change questions.
Include a Rust task and a Python task; the third project supplies another independent check of the same change procedure.
The projects may supply both outcomes when they meet all their conditions.

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
For each change, also keep a finding about independent code. It must remain usable while its own recorded inputs still match.
Restore the pinned starting files between independent cases so one change cannot hide another.
Also test a saved task resumed in another process after an edit.

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

## When this milestone is complete

Both outcomes need qualifying original results. The three projects must pass the full change procedure, including the first unread-project evaluation.
The Rust and Python tasks must pass their behavior, undo and separate-checkout delivery checks.
The catalog must link those results before either tracking requirement can close.
Passing unit tests or writing this plan does not finish the milestone.

Run builds, full tests and result collection on GitHub under the [workstation policy](development.md#working-on-a-shared-desktop).
This milestone checks correctness through changes. Measure speed and storage separately against fresh checks doing the same work, including setup costs.
It does not establish lower agent costs, broad package support, numeric translation or source-connected proofs.
See the [plan](../PLAN.md) and [current tracking](roadmap-status.md) for those larger goals.
