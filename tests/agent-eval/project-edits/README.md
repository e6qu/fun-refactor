# Four complete code changes for M2

The completed comparisons ask each selected agent to make four changes, with ordinary tools and with those tools plus fr.
Each setup gets two attempts per case: 48 attempts in total. Each attempt keeps the 150-second limit.
Each pair reverses its setup order in the second repetition.
The [task requirements](tasks.json) are recorded before extending tool support or running those attempts.

| Request | What a correct change does |
| --- | --- |
| Remove an unused Rust argument | Change the public function and its callers across two crates; preserve the result and report consumers the workspace cannot identify |
| Rename a TypeScript export through an alias | In `import { price as displayPrice }`, change the exported name to `quote` while keeping the local `displayPrice` name; leave the unrelated `price` function intact |
| Add an escaped-text byte count | Return how many UTF-8 bytes escaping needs, without constructing that text; `a.b` needs four bytes and `é` needs two |
| Include an invoice fee | Make `invoice_total(5, 2)` return 7 while preserving the public signature and unrelated source |

A parameter is an input listed in a function declaration. An argument is a value passed by a caller.
A literal argument is a direct value such as `false`; a call such as `record()` may have effects that removal must preserve or refuse.
A crate is a Rust package. An imported alias gives a local name to another module's exported function.
UTF-8 encodes characters as bytes; `é` occupies two bytes. Escaping `.` adds a backslash, making `\.` two bytes.
An allocation reserves heap memory. Callers use the byte count to reserve enough room before producing escaped text.

First check the task machinery with supplied correct and incorrect changes. Those are controls, not live agent successes.
Check the actual delivered patch separately in another checkout, including exact undo and redo.
Keep failed checks and timeouts. A completed CLI does not by itself establish a correct delivery.
Source changes during a review must invalidate its old saved preview before any write.

Implementation, current examples, deterministic checks, all 48 attempts, checked results and tracking belong in one PR.
The [M1 results](../selected-agents/local-results/README.md) stay intact; these new cases do not retry its failures.
Only patches and summaries from local model runs may be published. Full native logs and execution packets remain local.

## What the independent checks cover

The [language controls](https://github.com/e6qu/fun-refactor/actions/runs/40228056222) reject unchanged and incomplete answers, then accept a supplied correct patch.
The fr delivery checks exercise competing edits, failed-check diagnostics, source outside reviewed ranges and exact undo/redo.
The [private controls](https://github.com/e6qu/fun-refactor/actions/runs/40247933867) passed all twenty supplied cases without model calls.
Their compiler checks run without fr, runner credentials or external network access.
Regex checks cover 1,060 behavior cases, allocations, upstream tests and builds without default features in both packages.

Each private candidate keeps its 512 MiB memory, 60 CPU-second and 120-second elapsed-time limits.
A CPU-second measures execution on one core; waiting consumes the separate elapsed-time allowance.
Prepare unchanged dependencies once and report that shared time separately. Remove both changed packages from the cache so every patch rebuilds them.
Read immutable dependency libraries from the read-only cache rather than copying them into the memory-backed workspace.
That fixes repeated compilation and duplicate memory use without raising limits or supplying the new API.
The accepted regex control used about 93 seconds and peaked at about 452 MiB.

Twelve independent jobs check the published patches: one per agent and request, with four submissions each.
Initial grading uses the exact private image and compiler cache frozen before model calls.
Later CI rechecks prepare and test a fresh environment and record its identity separately.
All 48 outcomes must be checked, including patches delivered by timed-out clients.
A patch passing compilation cannot turn a timed-out client into an accepted attempt.

## The four completed comparisons

| Comparison | Accepted / 48 | Changes fixed before its first call |
| --- | ---: | --- |
| [Original](first-results/README.md) | 33 | Fr was available; its use was optional |
| [Shorter guides](guided-results/README.md) | 40 | State the deadline, require a useful fr step, simplify guide examples and repair the local Rust helper's library path |
| [Actual clock](clocked-results/README.md) | 41 | Present every requirement separately, state the actual finish time and reserve ten seconds for response and accounting |
| [Behavior checks](local-results/README.md) | 43 | Supply actual behavior checks, use an unoptimized development build and protect provided checking files |

Every comparison keeps all 48 scheduled outcomes. No individual failed slot was retried.
Each starts fresh projects and conversations with the same three models, settings, task requirements, order, repetitions and hard limits.
The studies combine several instruction and setup changes; comparing them cannot isolate the effect of one change.
Their within-study baseline is the same agent using ordinary tools on the same request.
They measure complete code-change time, not the changed application's execution speed.

The first comparison had 15 timeouts and 34 patches, including one from a timed-out client.
All submitted patches were correct; the late client still failed.
The report aggregation initially lacked an output directory. Its repair combined existing verdicts without recompiling.

The second comparison had five timeouts and three incorrect allocation changes.
Kimi's last fr regex patch was correct and arrived around 148 seconds, but its client missed the original deadline.
The third comparison had four timeouts and three incorrect allocation changes.
Both GLM fr regex patches passed the independent checks but their clients finished too late.
The fourth comparison closes those missing cases: every agent has an independently accepted fr result for every request.
All 48 submitted patches passed; five clients timed out and their attempts remain failed.

Some earlier shell-command counts miss calls after newlines; the exporter now recognizes newline-separated commands.
Computed executable names can still go uncounted. Reviewed contribution notes establish actual fr use.
Original counts and record hashes stay intact.

## Why the public behavior check changed

The earlier public regex command only checked compilation. It could accept the new APIs while missing the requested memory behavior.
For example, producing the right escaped text still fails the request when the string grows through multiple allocations.
Some agents added runtime examples to check values, but upstream development optimization level three made those builds unnecessarily slow.

The corrected public command runs actual byte, text, allocation and append checks against both libraries, then checks builds without default features.
It uses explicit development optimization zero and one compiler job. These are behavior checks, not speed measurements.
The [macOS controls](https://github.com/e6qu/fun-refactor/actions/runs/40273917213) reject missing APIs, wrong byte counts and excessive allocations, then accept the reference change.
The private grader and its broader independent checks remain unchanged.

Both tool setups receive the same command and checking files. Deleting or changing those files makes an attempt fail;
cleanup may remove only temporary files the agent created. This addresses two observed deletions of the entire supplied `.fr` directory.
The fourth comparison froze that environment and every slot before its first call, retaining the three earlier studies.

Preparation, model work, tools, patch delivery, response and native session accounting share the same 150-second deadline.
The supplied behavior command uses that attempt's original finish time, not a new allowance when launched.
Outer guard admission and controller Python startup precede the attempt timer; their time remains explicitly unmeasured.
Report matching successes, failed-attempt time, missing token usage, unknown account charges and reviewed fr contributions.
Later independent GitHub grading is outside the local attempt timer; its linked run retains those checking times separately.
M2 needs at least one accepted result with an actual fr contribution for every agent/request pair within the frozen repetitions.

The [independent grading run](https://github.com/e6qu/fun-refactor/actions/runs/40284596088) checked all 48 fourth-comparison patches.
The new public command exposed omitted allocation changes, allowing agents to correct them before independent grading.
Some corrections and reviews still finished too late. Other timeouts followed incorrect author inputs or workflow preview values.
These failures remain part of the comparison, even though their submitted code passed.

After collection, the guidance now names only the checking files actually supplied for a task.
It explains that refactoring JSON belongs in a fragment file and distinguishes the workflow's preview value from an author's preview value.
It also explains when to write an exact Git diff after workflow delivery, avoiding comparisons between equivalent patch formats.
Runnable examples check that wording; it is not another measured comparison.

Final review also found a Python rename bug: a function-local import could prevent an outer import and its calls from being renamed.
Current regression tests check the repair with ordinary Python execution before and after each rename, including local aliases and class methods.
The comparison still names the exact binary and source it measured. Its original auditor checks those frozen records;
current language and delivery tests check the later implementation changes. Original hashes and outcomes stay intact.
