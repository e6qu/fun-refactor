# Narrow grader reviews with provided source

One Kimi review completed, two calls timed out and three were not started. The frozen stop rule
ended this collection. Do not retry its cells or raise its limits. This is one reviewed question,
not acceptance of the three candidate tasks or evidence of an agent-efficiency advantage.

## What the models received

Commit `c93f14ee` froze the protocol, questions, inputs and six-cell allocation before any call.
Each question asks about one invariant and permits at most one finding. The initial packet contains
the requirement, complete grader and selected implementation spans. Reference repairs and mutation
controls are withheld. Other frozen source remains available through read-only tools.

The preparation script uses fr to locate and read declarations, checking bytes against frozen files.
Scoped method docstrings are omitted. The generic packet code checks hashes, extents, UTF-8 and
overlap, with 8 KiB of source and 12 KiB of encoded packet at most. Each source reference resolves
to exact delivered bytes. Citation validity does not establish the truth of a review claim.

| Question | Packet bytes | Source bytes |
| --- | ---: | ---: |
| dotenv: flat, nonrecursive alternate words | 6,714 | 5,378 |
| packaging: iteration, identity, order and duplicates | 6,454 | 5,038 |
| platformdirs: rejected relative paths must not create directories | 7,505 | 5,689 |

Calls retain the existing 120-second wall limit, 20 sampled CPU seconds, 768 MiB RSS and 16 MiB disk
limit. They allow 12 steps and 24 calls. Two consecutive failures stop the collection; retries are zero.
The outer workstation guard applies too. Candidate programs execute only in GitHub containers.

## Retained outcomes

| Question / model | Result | Wall seconds | Observed calls |
| --- | --- | ---: | --- |
| dotenv / Kimi K3 | Completed, one finding | 44.40 | One submission |
| dotenv / GLM 5.3 Flash | Timed out | 120.07 | One source read and one submission |
| packaging / Kimi K3 | Timed out | 120.08 | None; one unfinished step |
| packaging / GLM, platformdirs / both | Not started | — | — |

Kimi proposed that recursive expansion could pass the grader despite the required flat grammar.
For `BASE=yes`, `OTHER=expanded` and `OUT=${BASE:+${OTHER}}`, the requirement gives literal
`${OTHER}`. Recursive expansion instead gives `expanded`. The old grader lacked that distinction.

The candidate pack now preserves that exact old grader and adds a `flat-braces` case. A deliberately
wrong repair expands the resolved value again. GitHub job
[111408265330](https://github.com/e6qu/fun-refactor/actions/runs/37192769004/job/111408265330)
verified that the same candidate passes its public example and old grader, then fails only
`flat-braces` in the new grader. The reference passes both public and current private checks.
`counterexample.json` retains these two controls, source identities, grader hashes and artifact
provenance at commit `8a316d00`. Offline tests replay edits and check those bindings; CI also executes
the current controls afresh. This verifies one constructed wrong repair, not every possible repair.
The original model report still records citation-only validation; container evidence stays separate.

The review's two citations resolve to the requirement and the grader's last 317 bytes. They do not
directly quote the earlier literal-word case. Its limitations text also says the grader beyond byte
4413 was uninspected, although the complete 2365-byte grader was provided. Both statements remain
verbatim in the retained record. Provided context does not prove that a model attended to all of it.

GLM emitted a submission but did not finish the session export/audit. It remains failed, with no
accepted review. Packaging's unfinished step has no reported tokens; that does not imply zero cost.
All model calls made zero fr calls. Preparation-time fr use is separate from agent adoption.

## Accounting and replay

The completed review verifies its initial packet against the exported prompt. It retrieved no more
source, so its 5,378 provided source bytes are unique within the measured source context. The packet
is counted separately from the 18-byte submission acknowledgement and the answer. Failed exports
leave initial-context verification and combined source accounting unknown. GLM's retained read
contains 317 source bytes and its two results total 674 bytes.

Kimi's CLI reports 4,295 input, 603 output, 539 reasoning and 3,072 cache-read tokens. GLM's finished
steps report 8,376 input, 619 output, 4,953 reasoning and 3,264 cache-read tokens. Preserve provider
field semantics; do not add reasoning to output. Actual billing and complete provider context remain
unknown. A reported zero price is not a verified zero cost. No matched comparison was conducted.

`report.json` replays manifest identities, completed exports and partial failed streams. It does not
promote citations or submissions into behavioral proof. Run this bounded offline check under the
workstation guard; it makes no model calls and executes no candidate code:

```sh
python3 tests/agent-eval/opencode/reviews/2026-10-04-packets/collect.py check
```

Independent task selection, broad grader review, ordinary/fr comparisons and complete parent/child
costs remain open. The preceding stopped collections also remain stopped.
