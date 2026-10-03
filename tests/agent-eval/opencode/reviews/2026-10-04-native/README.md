# Source-based candidate review attempts

Neither review completed. Kimi and GLM both reached the 120-second wall deadline after reading
source, without submitting findings. The frozen stop rule prevented the other four planned calls.
These records do not accept the candidate tasks, verify a counterexample or establish an fr benefit.

| Requested model | Result | Tool calls | fr calls | Produced result bytes | Finished steps | Sampled CPU seconds |
| --- | --- | --- | --- | --- | --- | --- |
| kimi-code-plan-global/k3 | Wall timeout | 8 | 0 | 27,615 | 4 | 5.86 |
| zai-coding-plan/glm-5.3-flash | Wall timeout | 9 | 0 | 28,285 | 7 | 4.68 |

Both attempted `dotenv-alternate`. Packaging and platformdirs reviews were not started.
All 17 host results appear in the captured native streams. That establishes stream confirmation,
not complete provider context or billing. GLM supplied a stale hash on one read, received a refusal,
then continued with the correct hash. There were no candidate executions, edits or test runs.
Neither attempt called fr. The collector's largest sampled aggregate RSS was 672,137,216 bytes;
the workstation guard also remained active. No budgets were raised and no failed cell was retried.

## Frozen protocol and exposed inputs

Commit `5313cf5c` froze the collector, plan, citation controls and public-input provenance before
the first call. The plan identifies both executables, model IDs, runtime files, tool schemas,
six cells and source snapshots. Version detection, OpenCode and export share 120 wall seconds
and 20 sampled CPU seconds; the RSS ceiling remains 768 MiB. Tools are read-only, with 12 steps,
24 calls and bounded source pages. Ordinary reads and optional public fr exploration are available.

The existing native MCP server supplies selected upstream source, `review/requirement.txt`,
grader Python code, its case list and the public-check Python code. The source subsets are the
published candidate archives, not full upstream checkouts. Container setup is not exposed.
Reference repairs, mutation controls and the earlier review transcripts are withheld.
Every input was verified against nine public GitHub files at revision `91a41d7f`; see
`public-input-provenance.json`. Repository-specific identities live only in these evaluation inputs.

The requested answer permits at most two concrete findings, each naming a wrong repair,
distinguishing input, expected result and citations. Source IDs or exact quotes must resolve to
source delivered in earlier assistant steps. Contiguous reads work; invented text, unread gaps
and source fetched in the submission step refuse. A valid citation establishes what was read,
not whether the finding is correct. Findings require separate review and executable controls.
There are no completed findings in this collection.

## Replay and accounting

`inputs.json.gz` preserves the source snapshots identified by the pre-call plan. It lets replay
survive later changes to the candidate graders. Temporary workspaces and fr caches were removed;
the attempt directories retain streams, process measurements and file manifests. Original records
remain failed. No additional model request produces the derived `report.json`.

The shared `native_costs.observed(..., read_only=True)` path rebuilds ordinary tool responses and
checks disclosed fr source against the frozen files. It matches host results to native events,
counts repeated source and retains usage from finished steps, including failed attempts.
CLI-reported zero cost is not a zero-dollar bill. Both sessions have an unfinished step and lack
a completed export/model-identity audit; complete usage and provider billing remain unknown.

Run the offline report through the workstation guard locally:

```sh
python3 tests/agent-eval/opencode/reviews/2026-10-04-native/report.py --check
```

The [earlier source-reading cost report](../../../../../docs/native-read-outcomes.md) uses the
same accounting engine. It preserves that pilot's two passes and four timeouts. Its failed
attempts produced 33 tool calls and 100,968 result bytes; they are not zero-work failures.
Independent task review, a matched unfamiliar-project comparison and complete cost accounting
remain open.
