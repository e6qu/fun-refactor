# Review proposed repairs against their requirements

This collection asks whether the task author's proposed reference repairs violate specific
requirements. The source under `src/` includes the reference edits. Original pinned source is
available under `baseline/`; `review/reference.json` records the exact edits. A reference is a
proposal to inspect, not an accepted answer. The graders are also provided.

The three questions cover dotenv environment precedence, packaging's explicit prerelease policies,
and preservation of absolute XDG site paths. Earlier collections asked about other properties while
withholding reference repairs. Their stopped cells remain stopped; none is retried here.

`plan.json`, `inputs.json.gz` and this collector are committed before calls. The frozen plan allocates
Kimi K3 and GLM 5.3 Flash once per question, in that order. Each call keeps the existing 120-second
wall limit, 20 sampled CPU seconds, 768 MiB RSS and 16 MiB disk limit. Two consecutive failures stop
the collection. No retries or increased limits are allowed. The workstation guard also applies.

| Question | Packet bytes | Source bytes |
| --- | ---: | ---: |
| dotenv reference precedence | 6,221 | 4,794 |
| packaging reference policies | 5,523 | 4,123 |
| platformdirs reference paths | 7,923 | 6,095 |

Preparation uses fr to select declaration bytes and compares them with the exact repaired files.
Docstrings are omitted from initial excerpts; other frozen source remains available through read-only
tools. Calls cannot edit or execute candidate code. Source citations alone do not verify findings.
Any proposed counterexample needs separate behavior checking on GitHub before changing a grader.

Protocol 2 explicitly records reference disclosure in both the plan and report. Protocol 1 plans and
reports retain their original meaning and replay unchanged. Seeing a proposed solution changes the
review conditions; do not pool these calls with reviews that withheld it or with code-change trials.
Narrow reviews do not establish full task acceptance, independent task selection or an efficiency gain.

## Outcomes

Commit `6d3b5c79` froze the collection before calls. All six allocated calls were attempted once:
three completed Kimi reviews and three GLM timeouts. No budgets were raised and no calls were retried.

| Question / model | Outcome | Seconds | Calls | Tool-result bytes |
| --- | --- | ---: | ---: | ---: |
| dotenv / Kimi K3 | Completed, no scoped finding | 103.22 | 8 | 9,031 |
| dotenv / GLM 5.3 Flash | Timed out, no submission | 120.07 | 3 | 13,225 |
| packaging / Kimi K3 | Completed, finding rejected after verification | 100.04 | 1 | 18 |
| packaging / GLM 5.3 Flash | Timed out, no submission | 120.04 | 0 | 0 |
| platformdirs / Kimi K3 | Completed, no scoped finding | 72.93 | 4 | 9,840 |
| platformdirs / GLM 5.3 Flash | Timed out, no submission | 120.03 | 4 | 15,953 |

Tool-result bytes exclude the initial packets, instructions and submitted answers. Zero observed calls
does not mean zero provider work. Failed exports leave complete source context and usage unknown.
The report retains partial accounting from every timeout. Actual billing remains unknown.

Kimi's dotenv review inspected the interpolation precedence paths and reported no contradiction.
Its platformdirs review inspected the selected site-path functions and multipath property, also finding
no contradiction. Neither result accepts the whole task or proves the proposed repair correct.

## A completed finding can be wrong

Kimi claimed that `>1.0` accepts `1.0.post1`, so the grader should return that post-release instead
of `1.1a1`. The [packaging specification](https://packaging.python.org/en/latest/specifications/version-specifiers/#exclusive-ordered-comparison)
excludes post-releases of the bound's base version from that exclusive comparison. Version ordering
and specifier membership differ. The task also requires preserving individual comparisons.

The original model answer remains unchanged. `packaging-finding.json` records a separate rejection,
binding the finding, exact reference bytes, grader identity and GitHub artifact. The retained pinned
`_compare_greater_than` source explicitly rejects this post-release. GitHub job
[111504601805](https://github.com/e6qu/fun-refactor/actions/runs/37225639111/job/111504601805)
passed the same reference's `bounds-and-exclusions` case, including the disputed input and expected
`['1.1a1']` result. `packaging-controls.zip` preserves the original 6,696-byte artifact and its verified
SHA-256. Offline tests replay the source bindings and evidence; current CI reruns the controls.
The grader and reference were not changed to accommodate the incorrect finding.

## Observed fr refusal and remaining work

Kimi made one fr request during the dotenv review: behavior mode without a declaration handle.
It was refused and the agent continued with ordinary reads. This is attempted use, not successful
fr source retrieval. The other five calls made no fr requests. Preparation-time fr use is separate.

The new opt-in native tool schema 6 supplies an actionable names query after this specific refusal.
Its scripted controls preserve scope, source accounting and old-schema replay. These six calls used
schema 4; they do not measure whether the new recovery hint helps a live agent.

| Area | Review evidence | Still open |
| --- | --- | --- |
| dotenv | Earlier verified flat-grammar counterexample; current precedence review found no contradiction | Other parsing boundaries, complete task clarity and full grader review |
| packaging | Proposed policy finding rejected against source, specification and execution | Valid independent policy assessment, iteration and identity review, full grader review |
| platformdirs | Current absolute site-path review found no contradiction | User-path defaults, wrappers, side effects and full grader review |

Do not call these tasks independently accepted or start a live comparison on that basis. No matched
ordinary/fr comparison, independent task selection or complete provider-cost accounting is established.
The collection is finished. The earlier stopped collections remain stopped.

```sh
python3 tests/agent-eval/opencode/reviews/2026-10-05-references/collect.py check
```

Run this offline replay under the workstation guard. It makes no model calls and executes no candidate code.
