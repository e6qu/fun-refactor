# Four complete code changes for M2

The next comparison asks each selected agent to make four changes, with ordinary tools and with those tools plus fr.
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

## The three completed comparisons

| Comparison | Accepted / 48 | Changes fixed before its first call |
| --- | ---: | --- |
| [Original](first-results/README.md) | 33 | Fr was available; its use was optional |
| [Shorter guides](guided-results/README.md) | 40 | State the deadline, require a useful fr step, simplify guide examples and repair the local Rust helper's library path |
| [Actual clock](clocked-results/README.md) | 41 | Present every requirement separately, state the actual finish time and reserve ten seconds for response and accounting |

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
GLM and Kimi still need accepted, contributing fr results for regex before M2 can close.

Some earlier shell-command counts miss calls after newlines. Their reviewed contribution notes, rather than counts, establish actual fr use.
The exporter now recognizes those calls. Original counts and record hashes stay intact.

## Fix the supplied behavior check before another comparison

The earlier public regex command only checked compilation. It could accept the new APIs while missing the requested memory behavior.
For example, producing the right escaped text still fails the request when the string grows through multiple allocations.
Some agents added runtime examples to check values, but upstream development optimization level three made those builds unnecessarily slow.

The corrected public command runs actual byte, text, allocation and append checks against both libraries, then checks builds without default features.
It uses explicit development optimization zero and one compiler job. These are behavior checks, not speed measurements.
Separate macOS controls must reject missing APIs, wrong byte counts and excessive allocations, then accept the reference change.
The private grader and its broader independent checks remain unchanged.

Both tool setups receive the same command and checking files. Deleting or changing those files makes an attempt fail;
cleanup may remove only temporary files the agent created. This addresses two observed deletions of the entire supplied `.fr` directory.
A new comparison must freeze that checking environment and every slot before its first call, retaining the three earlier studies.

Preparation, model work, tools, patch delivery, response and native session accounting share the same 150-second deadline.
The supplied behavior command uses that attempt's original finish time, not a new allowance when launched.
Outer guard admission and controller Python startup precede the attempt timer; their time remains explicitly unmeasured.
Report matching successes, failed-attempt time, missing token usage, unknown account charges and reviewed fr contributions.
M2 needs at least one accepted result with an actual fr contribution for every agent/request pair within the frozen repetitions.
