# Four complete code changes for M2

The next comparison asks each selected agent to make four changes, using ordinary tools and then those tools plus fr.
Each setup gets two attempts per case: 48 attempts in total. Each attempt keeps the 150-second limit.
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
