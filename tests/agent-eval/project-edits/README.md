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

The [first independent controls](https://github.com/e6qu/fun-refactor/actions/runs/40228056222) passed for all four cases.
They reject unchanged and incomplete answers, then accept a supplied correct patch in a separate checkout.
Regex checks include Unicode byte counts, allocation counts, upstream tests and builds without default features in both crates.
These results check the task machinery. The 48 local model attempts have not started.

The fr delivery checks also exercise competing edits, failed-check diagnostics, exact bytes outside previewed ranges and undo/redo.
Private compiler checks run separately, with no runner credentials or external network access.
Their containers retain the existing 512 MiB memory, 60 CPU-second and 120-second execution limits.
A CPU-second measures time running on one CPU core; waiting still consumes the separate elapsed-time allowance.
Each patch gets its full language checks in a fresh checkout. Another copy verifies identical source and exact reversal without repeating compilation.
Prepare the unchanged regex dependencies once before private grading and record that shared preparation time separately.
Remove both packages under change from that cache. Every patch rebuilds them; cached results cannot supply the requested new API.
Read immutable dependency libraries from the read-only cache instead of copying them into the memory-backed workspace.
Keep writable copies of compiler state and regex-automata, which depends on the changed regex-syntax package.
Cache admission checks the original source hash, compiler identity and private image identity.
Before starting model attempts, record the successful controls, tool versions, private image identity and final instructions.

The [private controls](https://github.com/e6qu/fun-refactor/actions/runs/40235887167) passed all twenty supplied cases.
The accepted regex change used about 90 seconds of its original 120-second private-check allowance.
Earlier checks exhausted the allowance while compiling unchanged dependencies; shared preparation removes that repeated work.
It does not supply the changed crates or increase a candidate's allowance.

The public checker uses twelve jobs, one per agent and request, with four submissions in each.
An initial check uses the exact private image and compiler cache frozen before local model calls.
Later CI rechecks prepare and test a fresh environment; each run records its own image and cache identity.
That avoids depending indefinitely on a temporary download or relabeling a later environment as the original one.
All forty-eight outcomes must be checked, including submissions from timed-out CLIs.
The comparison reports matching successes, time spent on failures, incomplete token usage and actual contributing fr actions.

For M2, preparation and native session accounting share the same 150-second attempt deadline as model work.
The native process receives only the time remaining after preparation. Missing that deadline leaves a failed attempt, even with a valid patch.
Outer guard admission and Python startup precede the controller timer; report that unmeasured time explicitly.
Compare elapsed time and reported tokens against the same agent doing the same task with ordinary tools.
This measures the cost of completing a code change; it does not measure the modified application's execution speed.
