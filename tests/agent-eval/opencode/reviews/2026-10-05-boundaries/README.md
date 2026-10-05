# Review reference behavior at API boundaries

This collection reviews proposed repairs on three pinned Python projects. It covers different
properties from the completed precedence, explicit-policy and absolute-site-path reviews.
The earlier withheld-repair collections remain stopped. These calls disclose the proposed reference
and review its behavior; they do not restart those collections or establish resistance to wrong fixes.

The plan, source snapshots and collector are committed before calls. Each question gets one Kimi K3
and one GLM 5.3 Flash call, in that order. There are six allocated calls, no retries, and a stop after
two consecutive failures. Each call retains the existing 120-second wall, 20 sampled CPU-second,
768 MiB RSS and 16 MiB disk limits. The workstation guard also applies.

| Question | Initial packet bytes | Source bytes | Scope |
| --- | ---: | ---: | --- |
| dotenv reference regressions | 7,724 | 6,357 | Existing expressions, bare and adjacent substitutions, empty words, unassigned keys and disabled interpolation |
| packaging reference iterables | 5,523 | 4,123 | One-shot input, original objects, order, duplicates and empty specifier sets |
| platformdirs reference user API | 9,548 | 7,088 | Four user-home variables, defaults, suffixes, Path wrappers and directory creation |

Source under `src/` contains the proposed reference. Original source remains under `baseline/`;
`review/reference.json` records the exact edits. fr selects and verifies the provided source bytes.
The dotenv packet includes its full small grammar module. Short platformdirs methods keep their
docstrings; the long packaging method docstring is omitted. Other frozen source is available through
read-only tools. Neither local review calls nor replay execute candidate code.

This is the first reference-review collection using native tool schema 6. A behavior request without
a handle receives an actionable names query. The agent must choose whether to follow it; no search
runs automatically. Ordinary reads remain available. Changed tool availability alone establishes no
efficiency advantage. Source-cited findings still require separate verification against source and,
where behavior is disputed, execution on GitHub.

## Acceptance boundaries fixed before calls

These questions do not cover the complete requirements. The table separates scoped review evidence
from what is still needed for each task. A completed review with no finding is not a proof or approval.

| Task | Earlier evidence | New question | Still outside this collection |
| --- | --- | --- | --- |
| dotenv | Flat-grammar counterexample verified and grader repaired; precedence review found no scoped contradiction | Existing interpolation and malformed-expression regressions | Independent review of the repaired flat-grammar control, whole-task clarity and grader coverage |
| packaging | Policy finding rejected against source and execution | Input traversal and object-preservation contract | Valid independent policy assessment, unchanged contains/comparison behavior, full grader review |
| platformdirs | Absolute site-path review found no scoped contradiction | User-directory API defaults and effects | Site iterators and creation effects, unchanged runtime/media/non-Unix behavior, full grader review |

None of these candidate tasks is independently accepted. Independent task selection, matched live
ordinary/fr comparisons and complete provider-context and billing measurements remain open.
Timeouts retain observed work and unknown totals; a missing submission cannot become a passing review.

## Results

Not started. The committed allocation will retain every attempted call, including failures.

```sh
python3 tests/agent-eval/opencode/reviews/2026-10-05-boundaries/collect.py check
```

Run offline replay through the workstation guard after the report is retained.
