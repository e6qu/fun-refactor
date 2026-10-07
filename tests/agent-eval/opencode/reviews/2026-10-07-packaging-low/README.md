# Whole packaging review with explicit low reasoning effort

This fresh two-model collection keeps the complete question, eight-case grader
and source snapshots from `2026-10-06-packaging-inputs`. Both profiles now select
`low` reasoning effort with the same 2,048-token output allowance. Earlier
failed collections remain permanently stopped.

The plan binds the design, collector, provider catalog and hosted/workstation
admission evidence. All eight hosted transport controls and the single fresh
workstation control completed below 640 MiB. The collector requires a committed
plan and inputs before any model call. OpenCode uses its existing configured
authentication; the collector does not inspect or copy credentials.

Capture limits remain 120 seconds, 20 CPU seconds, 768 MiB sampled aggregate
RSS, 16 MiB disk growth and 1 MiB transcript. Local calls also use the outer
resource guard. The first resource-limit failure permanently stops collection.
Candidate code cannot execute during review. Model findings require independent
verification; empty findings do not imply whole-task acceptance.

The four ordinary-tools/fr code-change pilot attempts remain unstarted.

Both attempts again ended with `StructuredOutputError` and finish reason
`length`. Neither hit a resource limit. `stop.json` closes this collection.

| Model | Peak MiB | CPU seconds | Elapsed seconds | Final reasoning tokens | Final other output tokens |
| --- | --- | --- | --- | --- | --- |
| Kimi K3 | 504.19 | 4.81 | 86.20 | 1,724 | 324 |
| GLM 5.3 Flash | 528.45 | 3.47 | 63.48 | 1,327 | 721 |

Kimi also completed an earlier step with two ordinary source searches. Across
its two steps, the client reported 5,789 input tokens, 4,096 cached input tokens,
2,197 reasoning tokens and 418 other output tokens. GLM reported 4,477 input
tokens in its single step. Neither used fr. Both attempted a truncated structured
submission; GLM's partial arguments also misplaced required answer fields.
Client-reported cost was zero under subscription metadata; actual cost and
complete context accounting remain unknown.

Both partial answers predicted that the proposed reference would yield
`1.0.post1` for `SpecifierSet('>1.0,<2').filter(['1.0.post1', '1.1a1'])`.
That premise is incorrect: `_compare_greater_than` in the frozen source excludes
postreleases of the bound itself. Version ordering alone does not establish
specifier membership. The existing grader expects `['1.1a1']`.

[GitHub's candidate job](https://github.com/e6qu/fun-refactor/actions/runs/37591680438/job/112694404489)
passes all eight cases for this exact reference, including that assertion.
`claim-verification.json` binds the run and downloaded artifact;
`github-controls.json.gz` retains the complete controls. Offline replay rebuilds
the reference bytes and matches both its submission identity and the grader to
the model-visible inputs. This rejects the partial claim without upgrading either
failed review into a completed review or accepting the whole task.

The next review should divide the requirement checklist into explicit, bounded
questions and track completed coverage. It should require checking matching
implementation before alleging a comparison error. Raising resource or output
limits, retrying these cells, or accepting the task from these fragments would
not resolve the missing review.
