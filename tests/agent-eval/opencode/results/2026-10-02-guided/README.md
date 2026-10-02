# Compact exploration and optional public guidance

The plan froze before any of these eighteen calls. It compares ordinary source tools (`files`),
the same tools plus compact `fr project explore` (`fr`), and that identical tool surface with an
optional excerpt from the public exploration guide (`fr-guided`). Each of the three reviewed
Python tasks has one attempt per model and arm. Seed 409 fixes group and arm order.

OpenCode 1.18.34 used configured Kimi K3 and GLM 5.3 Flash. The CLI was the published macOS ARM64
`fr` 0.46.0 binary, 51,887,600 bytes. The plan records its SHA-256. No local build ran.
These are development trials on previously reviewed source and rubrics, not held-out evaluation.

| Task | Model | Ordinary tools | Compact fr | Compact fr + guide |
|---|---|---|---|---|
| cache-expiration | Kimi K3 | Pass, 78.7 s | Pass, 46.2 s | Citation failure, 69.5 s; 1 fr call |
| cache-expiration | GLM 5.3 Flash | Timeout, 120.1 s | Timeout, 120.0 s | Timeout, 120.0 s |
| key-rotation | Kimi K3 | Pass, 46.0 s | Citation failure, 48.3 s | Citation failure, 56.4 s; 1 fr call |
| key-rotation | GLM 5.3 Flash | Citation failure, 75.2 s | Citation failure, 96.1 s | Citation failure, 112.0 s |
| retry-callbacks | Kimi K3 | Citation failure, 75.3 s | Citation failure, 69.4 s | Citation failure, 70.3 s |
| retry-callbacks | GLM 5.3 Flash | Timeout, 120.1 s | Timeout, 120.1 s | Timeout, 120.1 s |

The original records show three passes, nine completed citation failures, and six stops at the unchanged
120-second wall deadline. All twelve completed answers supplied the four correct factual values.
The per-claim diagnostics in `summary.json` retain citation failures separately from value checks;
they do not change the original verdicts. Failed attempts lack a complete session export, so their
source and token totals remain unknown. Counts of recorded tool calls do not imply model delivery.

The separate `coverage-review.json` corrects an overly strict source-page rule. The original grader
required every quotation to occur in one returned page, even when the model had retrieved all its
bytes across adjacent pages. Joining verified, contiguous coverage recovers three answers: Kimi's
ordinary-tools retry answer, guided cache answer and guided key-rotation answer. The reviewed total
is six passes, six citation failures and six timeouts. Required factual values, anchors and quotation
bounds remain unchanged. Unread gaps, conflicting overlaps, different identities and later source
cannot supply evidence. This offline correction involved no new model calls or original-record edits.

Only two of twelve attempts with `fr` available called it. Both were guided Kimi attempts: one
names lookup for `TTLCache`, and one for `Signer`. Neither followed the behavior continuation.
The cache attempt tried an ordinary read at a nonzero offset without a hash, received a refusal,
and recovered by reading from the beginning. That interaction exposes a read-interface cost;
it does not establish why the model chose ordinary tools or why its final citation failed.

The tool schemas occupy 1206 bytes for ordinary tools and 1841 bytes for either `fr` arm.
The base instruction is 602 bytes; optional guidance adds 273 bytes including its heading.
The quoted excerpt itself is 235 bytes. Instructions appear both in the configured agent prompt
and the user task prompt. Each report records those sizes separately, plus result/source bytes
and CLI token categories where a full audited export exists. Hidden/provider context and actual
billing remain unknown. One repetition and six timeouts prevent a general efficiency conclusion.

Every attempt ran serially behind the workstation guard. Peak sampled process-group RSS was
685.05 MiB, below the inner 768 MiB cap. The outer guard retained its 1 GiB aggregate limit,
half-core CPU throttle, 2 GiB target limit and 64 GiB free-disk reserve. No limit increased.
The retained plan, runner, records and reports occupy about 4 MiB after disposable source and
grading workspaces were removed. No original trial record or older cohort changed.

`plan.json` binds the source snapshots, factual rubric, runner hashes, prompt, guide document and
excerpt, schemas and binary. `runner/` retains the exact implementation used. After the trials,
the current runner gained an early refusal for plans over 288 cells and accurate path labels for
alternative guide files. New plans also bind `contiguous-source-v1`; old plans retain their original
grading policy. The separate review records the corrected policy and auditor implementation. `attempts/` keeps root artifacts and integrity manifests.
`report.json` replays original verdicts; `summary.json` adds original per-claim diagnostics and counts.
`coverage-review.json` records the corrected coverage review separately.

From the repository root, replay without a model service:

```sh
python3 tools/native-rehearsal.py report \
  tests/agent-eval/opencode/results/2026-10-02-guided/plan.json \
  tests/agent-eval/opencode/results/2026-10-02-guided/attempts
```

To reproduce the separate correction, use `review` with the same paths and `--contiguous-source`.
On the local workstation, prefix these commands with the required resource guard. CI runs the
retained-record replay test without live model calls. This cohort closes a development experiment,
not an acceptance item: independent tasks, real change/proof grading, delegation and complete cost
accounting remain open. Improve source references and ordinary read access before another comparison;
do not force agents to use `fr` merely to increase its use count.
