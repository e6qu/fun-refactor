# Reported bugs and current limits

Use this page when fr refuses a command, gives a wrong answer, or cannot finish your task.
The [roadmap](PLAN.md#next-work) lists planned features. A missing feature and an incorrect answer need different reports.

## Merged repairs and remaining limits

The [supplied-proof delivery review](tests/agent-eval/proof-algorithms/local-results/M3-delivery/review.md)
found six OpenCode2 2.0.32 sessions whose saved final response has token counts but
whose JSON stream omits that final usage event. Correct patches from those sessions
remain accepted patches; their complete attempts remain failed. The exporter already
rejects incomplete usage. The local runner and native admission now reject missing
or duplicate response accounting immediately, rather than initially reporting native
completion. A no-model reproducer confirms that the client stops reading ordinary
events after session completion and restores text and tools without final usage.
Both stream shapes fail with 2.0.32 and pass with the official 2.2.5 fix.
The project pins 2.2.5 and requires complete native accounting.
The evaluator reconstructs no missing usage, and original outcomes remain unchanged.

The same review retains one Kimi timeout after both saved proofs and the package
passed. It used unsupported `fr history --limit` commands instead of exporting the
saved IDs. Fr correctly rejected that flag. The failure does not justify a longer
deadline or adding an unused option to fr.

The 0.88 release changed executable hashes while a current project-change replay
still recorded the exact 0.87 binaries. The branch now includes the release, and
the [hosted replay](https://github.com/e6qu/fun-refactor/actions/runs/41651284593)
passed all 21 saved-findings changes and both deliveries again.
New retained records match the released build; original records and exact hash
checks remain intact.

The Python text-flow, structural-change and consumer-scope checkers also hashed
the SDK release literal as behavior input. They now use the shared release identity rule.
Structural and scope checks bind the complete SDK tree and packaging metadata too.
Matching release updates preserve
input identity; real SDK changes and mismatched SDK/native versions still invalidate it.
A regression covers all three cases. The
[fresh language checks](https://github.com/e6qu/fun-refactor/actions/runs/41659946220)
and [authenticated retention](https://github.com/e6qu/fun-refactor/actions/runs/41661372321)
pass. New records replace the stale current entries without changing original results.

An earlier [scripted native check](https://github.com/e6qu/fun-refactor/actions/runs/41651295817)
failed four OpenCode exchanges but retained only a generic outer error.
CI now retains bounded stderr and all scripted exchange diagnostics.
A [later failure](https://github.com/e6qu/fun-refactor/actions/runs/41661802399)
shows that the server can listen before its local plugin service exposes agents.
It returns a specific 503 response before any model or Lean call.
The agent-readiness loop now waits for that state within its original deadline.
It rejects authentication errors, unrelated 503 responses and malformed replies immediately.
A persistent startup failure still expires; no timeout increased.
The earlier lost inner error remains unknown, and every failed check stays recorded.

The streaming replay, native-protocol check and configured-result grading
exhausted their five-minute job limits while checking out about 81,000 repository
files. Those jobs and the configured-change controls now fetch only the scripts,
actions and retained fixtures they use. Focused tests and archived-result checks
pass in actual sparse checkouts. Hosted streaming and native-protocol jobs pass;
every five-minute limit stays unchanged.

Five Rust and Python test groups exhausted their fifteen-minute limits after long
full checkouts. Each group now fetches records named by its assigned tests,
their Python tools and their current catalog entries. SDK checks also retain the
records used by their Python tool checks.
A regression fetches required records into a test repository and verifies their bytes.
It also checks that Git leaves out unrelated archives.
The complete test inventory and all deadlines remain unchanged.
CI monitoring must inspect cancelled jobs too: GitHub labels these timeouts cancelled.

The narrow checkouts initially missed JSON file lists, documentation links and
records that SDK tools read. The checkout now includes these files and the artifacts
named in catalogue records. A second checkout restores shared task files correctly,
so patch replay can read its tasks after loading the recorded source revision.
CI checks old indexing measurements with their original checker in a temporary
Git checkout. Git fetches the required files together.
The regression tests build small Git repositories, switch revisions and check the original files.

Repeated native checks failed before any client ran because GitHub returned HTTP
500 for the pinned Codex companion download. The installer now retries that same
URL at most three times within its original thirty-second request deadline.
Authentication errors, missing files and hash mismatches still stop immediately.
Scripted admission uses the current installer; historical grading restores its original source.

Earlier repairs landed in [PR #592](https://github.com/e6qu/fun-refactor/pull/592) and
[PR #593](https://github.com/e6qu/fun-refactor/pull/593).
PR #593 passed its [complete CI gate](https://github.com/e6qu/fun-refactor/actions/runs/41595629760).
The interrupted algorithm batch exposed these defects; their original failed
attempts remain failed after a repair:

| Problem | Repair and remaining check |
| --- | --- |
| The inner memory and CPU monitor missed OpenCode's detached private server | Follow descendants across process groups and stop observed workers on exit. Focused tests pass locally and in [hosted qualification](https://github.com/e6qu/fun-refactor/actions/runs/41398715778) |
| Killing the controller could lose buffered native feedback | Flush every received block. A regression checks that partial output is readable while the controller is still running |
| Fr rejected induction over two inputs before Lean ran | The generated reader accepts both inputs and the named induction rule. Native qualification passes; both fixed-corpus audits preserve the reviewed facts |
| CPU throttling could pause the inner sampler while a new worker ran | Observe the whole required guard and workload from outside the throttled tree. Keep all limits unchanged and preserve the original controller report |
| Initial-goal admission missed later proof and package memory | Both GCD task files now disable the automatic library import. A complete scripted prototype fits the fixed budget. The freezer now requires full-workflow receipts with client-state headroom; hosted qualification and all three complete native-client checks passed before the twenty fresh sessions |
| An interrupted result filled unknown system sleep with zero | The outer observer now compares both clocks, including when the controller is stopped. Keep the earlier result unchanged and label its missing sleep measurement unknown |
| Lean accepted a multiline function body that fr rejected | Both executable-GCD references now pass, including a continued argument. Tests keep the following tactic and declaration separate. Original failed agent work stays failed |
| Recursive definitions and local helpers confused the reader | The hosted algorithm checks accept Euclid's termination hints and isolate local-helper renames. Preserve the exact benchmark-fact audits |

The [interrupted batch](tests/agent-eval/proof-algorithms/interrupted-results/README.md)
remains inconclusive. The exact allocation behind its outer memory stop is unknown;
the repair must not be described as a measured reduction in client memory.
The [later Init session](tests/agent-eval/proof-algorithms/interrupted-init-results/README.md)
also remains a failed, incomplete comparison. The later twenty-session study is complete;
see the [results review](tests/agent-eval/proof-algorithms/local-results/review-context.md).
One GLM presentation attempt hit a multiline-lambda reader defect: Lean accepts
a function argument continued on the next line while measured fr rejects it.
The reader repair has a valid GCD reference control and checks following declarations.
Keep the original failed agent draft separate; it also contained Lean errors.
The project replay also hit its CPU ceiling after accounting improved.
Its [single-worker replay](https://github.com/e6qu/fun-refactor/actions/runs/41401084179)
passes with the original CPU and wall budgets. Both the collector and CI now select one worker explicitly.
The [package replay](tests/agent-eval/package-questions/cpu-limit-review.json)
exposed another boundary: its fresh worker environment discarded the caller's worker setting.
Nine Boltons cases exhausted the unchanged CPU budget. The worker now selects one
Rayon worker explicitly. All nine controlled replays pass below the same limits;
the largest sampled CPU total is 7.46 seconds against the original ten-second budget.

A separate Mac reasoning control stopped at 825,114,624 sampled RSS bytes,
above its unchanged 805,306,368-byte ceiling. Its client now uses the explicit
smaller Bun runtime setting. The [scripted reasoning controls](https://github.com/e6qu/fun-refactor/actions/runs/41434050053)
pass on Mac and Linux. These controls make no model calls and grant no live-agent admission.

The additional reader comparison exposed a partial-checkout error. Its worktree
inherited CI exclusions and omitted archived sources. The
[probe review](tests/agent-eval/index-consumers/lambda-audit-preparation-review.json)
preserves each failure. The checker now requires every tracked source before indexing.
Both [complete fixed-corpus audits](tests/agent-eval/index-consumers/lambda-reader-control-summary.json)
pass and preserve the recorded source facts. Partial inventories cannot qualify a complete-source claim.

The earlier repairs below belong to merged M3 work, including PR #591.
The measured tool passed all 29 proof-preparation jobs and the 17-job deep audit
before the earlier complete comparison. Those counts do not qualify the current changes.

| Earlier problem | Current branch behavior |
| --- | --- |
| Agents could not inspect progress between proof steps | Adds `proof-state` with the original claim, available assumptions, goals and draft cursor; distinguishes a solved branch from whole-proof acceptance. [Hosted branch tests](https://github.com/e6qu/fun-refactor/actions/runs/41348371660/job/124973004157) and the packaged skill examples pass; inspection cannot issue a proof receipt |
| A short proof-inspection deadline could expire immediately | Dependency preparation and editor inspection now share an absolute deadline. Fractional time remains usable; expired work cannot start or report success. [Hosted deadline checks](https://github.com/e6qu/fun-refactor/actions/runs/41348371660/job/124973004148) pass without increasing any limit |
| The filesystem check treated a Unix-only test module as runtime code | Recognizes `all(test, ...)` conditions while keeping runtime alternatives checked. [All six filesystem checks](https://github.com/e6qu/fun-refactor/actions/runs/41347972509/job/124971785531) pass, including the runtime-alternative regression |
| Valid Lean proof forms were rejected before checking | Accepts discarded assumptions such as `rintro ⟨h, -, -⟩`, simplifier phase markers, Unicode lambda arrows, branching bindings and dated attributes; checks later declarations too |
| A valid inline calculation was rejected before Lean ran | All 29 hosted preparation checks pass. A replay with the rebuilt executable accepts the earlier valid draft and keeps the invalid draft rejected by Lean |
| Tagged Lean errors lost the draft line and column | All 29 hosted preparation checks pass. An exact replay now reports both unknown names at their submitted lines and columns. Lean counts columns from zero; fr converts them to columns starting at one and preserves first-column errors |
| The page-count evaluator crashed on a known-correct patch | It now reads settings from the selected study directly. Its regression reproduces the old crash. All seven hosted patch controls and four scripted client exchanges passed before agent attempts |
| Nested assumption patterns were rejected before Lean ran | All 29 hosted preparation checks pass. The rebuilt reader accepts nested patterns such as `⟨⟨a, b⟩, c⟩`; replaying the original draft now reaches Lean and exposes its remaining proof errors |
| Lean propositions were grouped incorrectly | Reads `p ∧ q ∧ r` as `p ∧ (q ∧ r)` and gives disjunction its proper precedence |
| Library lookup lost a declaration's type | Keeps declared types for equation-style definitions and proofs; returns bounded headers without proof bodies |
| A draft error pointed only to generated code | Reports the draft line and source excerpt; retains the actual goal and counts omitted context |
| A valid local Lean lemma with parameters was rejected | Generated reader checks and focused native examples pass for named and anonymous parameterized lemmas; full workflow qualification continues |
| A draft passed fr but its warnings later failed package verification | Rejects draft warnings before saving; a clean proof remains checkable while another theorem is unfinished; focused native checks pass |
| A report calculator accepted late successes or invalid times | Checks elapsed time against the recorded deadline and rejects invalid times; twelve retained complete reports keep their original statistics |
| The small proof diagnostic skipped shared execution instructions | Uses the common instructions for check-file preservation, serial work and finishing; a regression rejects the measured omission |
| Rebuilding the original proof auditor omitted required task files | Both checkouts select the same explicit sparse mode; the early input check and all eighteen isolated controls now pass on GitHub |
| An application update deleted a frozen CLI executable | New studies keep verified executable and companion copies inside the study directory |
| Deep conformance omitted TypeScript's compiler | Installs the locked checker before running the unchanged checks |
| Repository reports repeated file reads and line scans until timeout | Reuses source/line maps within a report and the immutable test graph within a test; preserves Unicode positions, fresh later reads and every original sample |

For example, `名` occupies three UTF-8 bytes but advances a reported character column by one.
Both frozen repository reviews retain the current reviewed declarations and references.
Installed Lean-library lookup still reports syntax gaps; an empty search in that state cannot establish that a fact is absent.

The latest investigation also found two reporting defects:

- Adding an archived result changed the identity of an otherwise unchanged proof checker.
  The identity now covers checker code, settings, fixtures and control proofs, with a regression for archives.
- The final independent-result job exhausted five minutes while disabling its limited checkout and fetching the whole repository.
  It now keeps only the original checker and required inputs. Rebuilding the original auditor also excludes the large report archive.
  Shared receiver fixtures, including Boltons, remain present. The corrected collector finished in 22 seconds.
  The deadlines, proof checks and original agent outcomes stay unchanged.

The proof gate also searched the whole project for a function whose file it already knew.
That read 2,933 files and began resolving 871,433 references before its three-minute limit.
The query now selects `src/spec.rs` explicitly and checks the returned name, signature and one-file coverage.
The proof checks and repository audits retain their original limits and coverage.

The preceding comparison's nine checking groups accepted all 27 submitted patches; nine attempts timed out without one.
The new complete comparison retained eight timeouts and 28 independently accepted patches.
Its accepted access and range proofs use actual fr checks and writes. M3 still lacks GLM and Kimi page-count proofs.
The [behavior review](tests/agent-eval/proof-writing/behavior-review.md) records the exact mistakes and timing.
The root skill now links arithmetic directly and starts by checking an unfinished draft.
The lesson explains Boolean comparisons, proof-valued arguments and a search for a missing fact.
All four examples compile with pinned Lean; the complete skill passes hosted M2 checks.
The [small diagnostic](tests/agent-eval/proof-feedback/README.md#what-the-first-run-established) retained eight valid proof patches and six completed attempts.
GLM timed out after passing checks; Kimi replaced the check settings with a report. Both remain failed.
The [later construction check](tests/agent-eval/proof-construction/README.md#what-the-first-run-established) used the restored instructions and kept all four settings files unchanged.
Three patches passed independent grading; one Kimi attempt timed out without a patch.
Its repeated inline-calculation rejections led to a reader defect reproduced without model calls.
Full comparisons stay paused while the [improvement method](docs/agent-improvement.md) guides this repair and the next question.
The [page-count error review](tests/agent-eval/proof-page-count/draft-error-review.json) separates syntax, assumption grouping and missing division facts.
Its final manual control proves the unchanged claim. GLM and Kimi still lack complete accepted page-count attempts within 150 seconds.

The current CI repair also separates the slow formal-kernel tests from the other native tests.
Eight workers retain every test exactly once and keep the existing 15-minute deadline.
All eight native groups and four Python SDK groups passed in [CI](https://github.com/e6qu/fun-refactor/actions/runs/41312682461).
One SDK runner also exhausted disk while installing test tools, before tests started.
Native setup declares a 16 GiB reserve and removes unused Android and .NET SDKs only when it needs space.
It logs free space before and after, then stops if the reserve remains unmet.
These native and SDK checks passed after the disk repair; the remaining independent CI checks continue.
The next diagnostic also exposed a build-verification defect after a partial workflow rerun.
GitHub's default job listing omitted the earlier successful builder.
The verifier now requests every job page and still requires one unambiguous successful builder with matching source inputs.
The study checkout also omitted a newly linked result manifest.
It now selects evidence paths from the guides alongside the audit catalogs.
A small checkout control restores the linked manifest while leaving its source archive out.

Also check the [open issues](https://github.com/e6qu/fun-refactor/issues) before reporting one. This list cannot establish that the tool has no bugs.

Report a small program when a supported input produces the wrong result.
Include the command, explicit settings, expected result, actual output, fr version and source revision.
For a write failure, also keep the saved transaction ID and say whether source files changed.

An unfinished trace leaves a question unanswered.
Reporting that unfinished trace as a complete negative answer is a correctness defect.

## Choose what to do after a failure

| Symptom | What to inspect | Next action |
| --- | --- | --- |
| A rename refuses before writing | The stated refusal reason and [command reference](CLI.md) | Resolve the ambiguity or report a supported input that should work |
| A trace has no reported connection | Completeness, unread code and cutoff reasons in [result meanings](docs/understanding-results.md) | Recheck unsupported behavior before concluding there is no connection |
| A check fails after applying an edit | The saved transaction and failed stage | Follow [repairing an applied change](docs/source-repair.md) |
| A write stops partway through | The pending recovery state | Follow [file recovery](docs/host-recovery.md) before another write |
| A cached answer disagrees with a fresh one | The source files, settings and tool inputs recorded behind it | Keep both answers and report the mismatch |
| A command reaches a work or time limit | The last completed work and declared limit | Identify repeated or unnecessary work; keep the limit |

A transaction is a saved source change. Its returned ID lets you inspect, undo or recover that change.

## B5: Calls supplied from outside the project can remain unknown

A caller might receive a function from another program, load a plugin at runtime, or construct a name from data.
Source files alone may not identify the function that will run.

For example, `run(callback)` calling `callback()` cannot name one target when the project never supplies that callback.
An untyped call such as `shape.area()` can have several possible targets with the same method name.

Fr uses written assignments, imports, annotations and declared type relationships to narrow the possibilities.
Its reports retain uncertainty when those facts do not settle the target.

| Report label | What it means for your task |
| --- | --- |
| `function-value` | Source code assigns a particular callable value to the name |
| `receiver-type` | Source evidence narrows the object before the method call |
| `field-based` | Several functions can occupy the named field; the program still chooses at runtime |
| `method-name` | The report found matching method names without a declared type relationship |

A receiver is the object before the dot in `shape.area()`.
Review external callers, plugins and unread files before using an unused-code result to justify deletion.
The [boundary tests](tests/analysis_boundaries.rs) check both the uncertainty and the report that explains it.
The [grammar tests](tests/known_grammar_gaps.rs) check syntax repairs and nearby supported forms.

## Planned features

The [active roadmap](PLAN.md#next-work) names the selected agent setup, concrete refactorings and repairs, and Lean proof-writing tasks.
Invalid-number conversion and shared-list analysis are outside the active queue unless one of those tasks needs them.
Their unfinished earlier claims remain in the [qualification checklist](docs/earlier-roadmap-checklist.md).
Keep current task requirements and completion checks in PLAN.md.

## Closed defect history

The [pre-cleanup bug history](https://github.com/e6qu/fun-refactor/blob/752af561fbd698f598f2786d3957c3d7e68a3180/BUGS.md#fixed)
retains the closed entries and original repair notes.
Use [CHANGELOG.md](CHANGELOG.md) for release history and Git history for the associated changes.

When a defect closes, keep its reproducer as a behavior test and record the repair in its PR.
Keep this page focused on problems a contributor can act on now.

