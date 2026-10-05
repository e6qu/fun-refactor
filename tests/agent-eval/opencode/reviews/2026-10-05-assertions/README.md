# Review one assertion with unchanged files identified

This collection reviews one specified assertion per candidate task. It discloses the author's
proposed reference repair; that repair is not a proven answer. Earlier failed collections remain
stopped. These calls do not resume their broader questions or establish whole-task acceptance.

The collector, plan and inputs are frozen and committed before the first call. Calls use the
configured OpenCode Kimi K3 and GLM 5.3 Flash profiles, in that order for each question below.
The fixed order begins with object identity, then directory creation, then absent-variable expansion.

| Question | Assertion being reviewed | Initial packet bytes | Included source bytes |
|---|---|---:|---:|
| packaging-version-identity | Two matching prerelease `Version` objects survive as the same objects, in order | 4,197 | 2,616 |
| platformdirs-rejected-directory | Reading `user_config_dir` with `ensure_exists=True` creates no directory from `rejected-relative` | 4,462 | 2,609 |
| dotenv-absent-alternate | Absent `BASE` turns `before${BASE:+chosen}after` into `beforeafter` | 6,966 | 5,157 |

Each packet contains the full requirement, the exact relevant grader excerpt and selected reference
source. The question explicitly excludes adjacent cases. The directory excerpt retains the preceding
assertion that accesses the property, as setup for the assertion under review.
The long `SpecifierSet.filter` docstring is omitted; its executable body remains intact.
The small dotenv variables module is included whole so its expression grammar is visible.
Source extents are checked against the installed fr binary. Ordinary source tools remain available.

`review/file-comparison.json` identifies two selected before/after file pairs per question.
Each pair records paths, full-file SHA-256 hashes, byte lengths and executable flags.
One pair changes and one is byte-for-byte unchanged in each packet. This is selected-file context,
not a complete diff or a claim that identical bytes behave identically at different paths.
The comparison is itself frozen packet source and counted in the supplied source bytes.

The schema-6 adapter retains the existing recovery hints. Limits stay at 120 wall seconds,
20 sampled CPU seconds, 768 MiB RSS, 16 MiB retained disk, 12 steps and 24 tool calls per attempt.
There are no retries. Two consecutive failures stop the remaining cells. Each local call also
runs under the stricter machine guard. Candidate code and graders execute only on GitHub runners.

Run `collect.py report` to derive the review and source-reuse reports, or `collect.py check` to
recompute and compare both. Both include failed attempts. Full provider context and billing remain
unknown; a completed answer alone is not a verified counterexample or an efficiency result.

## Results

Frozen commit: `c938151b206a3df63d7c1bd067b76bcbb135a844`, pushed before any calls.
The collection is stopped: **three completed, two failed, one not started**. Do not resume it.

| Question | Model | Result | Seconds | Host calls | Additional source bytes |
|---|---|---|---:|---:|---:|
| Version object identity | Kimi | Completed, no scoped finding | 33.96 | 1 | 0 |
| Version object identity | GLM | Completed, no scoped finding | 64.74 | 5 | 584 |
| Rejected directory | Kimi | Completed, no scoped finding | 21.32 | 1 | 0 |
| Rejected directory | GLM | Failed: answered as text without submitting | 36.53 | 0 | 0 |
| Absent alternate | Kimi | Failed: submitted four times; three refusals | 37.74 | 4 | 0 |
| Absent alternate | GLM | Not started after two consecutive failures | — | — | — |

The completed reviews cover two assertions, not two whole tasks. Both packaging reviewers traced
original objects through `pending` and `yield from pending`. Kimi's directory review traced the
rejected relative value to the existing absolute fallback before directory creation.
Neither result establishes general repair correctness or a complete grader review.

GLM's directory response contains a readable no-finding explanation, but it never called
`submit_answer`. Kimi's dotenv attempt made one accepted submission and three duplicate calls.
The host refused all duplicates; the audit rejected calls after submission. Keep both failures,
their completed-step usage and the final unstarted cell. Do not recover an answer from ordinary
text, accept only the first submission, or retry these cells.

All five attempts ended within their budgets. There were 11 host calls, 2,800 result bytes and
584 retrieved source bytes. No attempt called fr. GLM's packaging reads repeated 76 source bytes;
including overlap with the initial packet raises that to 110 bytes. The separate exact-file audit
found no additional cross-path repetition. None of these numbers proves the file comparison
caused fewer reads: question scope and supplied source changed too, with no matched control.

Local guard CPU measurements ranged from 4.23 to 6.20 seconds per attempt. Those samples omit
provider work. Full context and billing remain unknown. All recorded process outcomes, including
the failed reviews' exported sessions, are retained for replay.
