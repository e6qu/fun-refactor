# Twelve OpenCode repository explanation attempts

One of twelve attempts passed the frozen factual and retrieved-source grade. One completed
answer failed the answer contract; ten attempts did not complete. This comparison provides
no evidence that fr improves agent efficiency.

The run used OpenCode 1.18.34 with the configured `kimi-code-plan-global/k3` and
`zai-coding-plan/glm-5.3-flash` profiles. Each model tried three pinned Python repositories
with ordinary source tools and with the same tools plus fr. The seed was 409, with one
attempt per cell in frozen order. No failed cell was retried or replaced.

| Repository behavior | Model | Tools | Outcome | Complete responses | fr calls | Disclosed source bytes |
|---|---|---|---|---:|---:|---:|
| Cache expiration | GLM | fr available | Non-action response | 3 | 0 | 4,221 |
| Cache expiration | GLM | Files | Truncated session export | 7 | 0 | 23,150 |
| Key rotation | Kimi | Files | Turn budget exhausted | 8 | 0 | 16,611 |
| Key rotation | Kimi | fr available | Non-action response | 5 | 0 | 25,210 |
| Retry callbacks | GLM | fr available | Turn budget exhausted | 8 | 7 | 1,334 |
| Retry callbacks | GLM | Files | Turn budget exhausted | 8 | 0 | 26,624 |
| Key rotation | GLM | fr available | Truncated session export | 6 | 0 | 25,210 |
| Key rotation | GLM | Files | Non-action response | 3 | 0 | 15,563 |
| Cache expiration | Kimi | Files | Passed | 6 | 0 | 10,209 |
| Cache expiration | Kimi | fr available | Non-action response | 3 | 2 | 4,096 |
| Retry callbacks | Kimi | Files | Answer contract rejected | 6 | 0 | 22,096 |
| Retry callbacks | Kimi | fr available | Non-action response | 3 | 0 | 2,714 |

The five non-action failures returned no single valid JSON action. Three attempts exhausted
the eight-response limit. Two final answers reached export, where OpenCode truncated JSON
through a pipe. The completed retry answer supplied the expected four Boolean values, but
added a `notes` field and used an abbreviated quotation containing an ellipsis. The original
grader rejected the extra field before checking facts or citations. Calling this a factual
error would misdescribe the evidence. Future instructions state the strict format explicitly.

Only two of the six fr-available attempts chose fr. They issued nine calls altogether; both
failed to finish. Fewer source bytes in a failed attempt do not establish an efficiency gain.
The host implements a constrained JSON exchange, not native OpenCode tools, the full fr skill,
edits, compilers, proof tools or delegation. Protocol friction dominates this run.

## Original evidence and later corrections

`plan.json` binds source bundles, private rubrics, grader, six runner modules, binary hash,
instructions, model IDs and budgets. `frozen-runner/` contains those exact modules and the
grader. The developer reused the existing fr 0.35.0 binary, 121,904,816 bytes, without a build.
This was not a run against a newly compiled current checkout.

`attempts/` retains every raw CLI stream, parsed action, tool result, final answer, available
session export, grade and process sample. Per-attempt manifests bind those files. Source
bundles appear once in the plan; each submission refers to its source hash. Source snapshots
and licenses also remain in the [archive inventory](../../repositories/README.md).

`original-report.json` preserves the first audit. It incorrectly counted a tool result produced
after the final allowed turn as disclosed. `report.json` excludes such results unless a later
prompt contains them. `audit-review.json` binds both reports and `reviewed-disclosure.py`.
Three attempts produced an undelivered final result; two included source bytes. The correction
removes 7,814 source bytes and changes no outcome, token counter, transcript or timing.

The original cache export stopped at 65,536 bytes. A later read-only export of that existing
session produced 66,953 valid bytes through a regular file, with matching model/turn identities.
`export-diagnostic.json`, `export-recovered.json` and `reviewed-export.py` retain that check.
The first helper used a process-wide file-size limit, which blocked OpenCode's SQLite checkpoint;
`export-wrapper-refusal.json` retains the refusal. The corrected helper samples only the export
file's 1 MiB cap inside the monitored attempt directory. It preserves the outer resource limits.
Neither diagnostic made a model request or replaced an original failed outcome.

## Resources and remaining gaps

All attempts ran serially under the workstation guard, with unchanged limits: eight responses,
120 seconds per attempt, 45 seconds per subprocess, 768 MiB sampled process-group RSS and
20 sampled CPU seconds per subprocess. The outer guard also enforced its 1 GiB aggregate RSS,
CPU throttle, disk floor and deadline. Summed attempt time was 565.2 seconds; this excludes
development and gaps between attempts. The largest sampled subprocess-group RSS was 645.1 MiB.
The final local check found 103.7 GiB free disk and 543.5 MiB of logical target contents.
Retained records occupy about 2.3 MB. No local full build or repository test gate ran.

Reports preserve CLI input/output/reasoning/cache counters and exact source overlap. They do
not establish complete context, verified provider billing, global OpenCode storage growth,
escaped-process resources, formal proofs or unfamiliar-project success. Developer review of
the rubrics and possible model training exposure also limit independence.

Offline tests replay all twelve attempts and regrade both completed answers. CI independently
executes boundary, ordering and callback controls against the three pinned libraries. CI does
not contact either model provider. See the [method](../../../../../docs/opencode-source-evidence.md).

The next useful comparison should reduce the observed interaction and answer-contract friction
before spending more calls. It still needs repeated tasks, useful changes and source-connected
proofs, delegation, and complete resource/context measurements. The roadmap remains at seven
of eighteen demonstrated acceptance items and zero of four completed milestones.
