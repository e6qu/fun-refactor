# Whole packaging review with provider metadata retained

This fresh two-model collection uses the same complete packaging question and
eight-case grader prepared in `2026-10-06-packaging-inputs`. The earlier empty
catalog collection remains permanently stopped with its failed Kimi capture and
unstarted GLM capture intact.

The plan binds the provider-preserving catalog, collector implementation and
the hosted/workstation evidence in `memory/2026-10-07-providers`. Both platform
experiments passed admission; the workstation source-delivery and endpoint
checks passed before this plan was frozen. Plan and inputs must be committed
before any model call. The collector enforces that condition.

Capture limits remain 120 seconds, 20 CPU seconds, 768 MiB sampled aggregate
RSS, 16 MiB disk growth and 1 MiB transcript. The local resource guard also
applies. The first resource-limit failure permanently stops the collection.
Candidate code cannot execute. Findings need independent verification, and
empty findings do not constitute whole-task acceptance.

Both providers responded, but both attempts failed with `StructuredOutputError`
and finish reason `length`. Neither produced a completed review or called a tool.
This collection is permanently stopped; its failures cannot be retried.

| Model | Peak MiB | CPU seconds | Elapsed seconds | Input tokens | Reasoning tokens | Other output tokens |
| --- | --- | --- | --- | --- | --- | --- |
| Kimi K3 | 453 | 3.49 | 53.35 | 4,276 | 2,048 | 0 |
| GLM 5.3 Flash | 433 | 3.38 | 47.35 | 4,478 | 2,047 | 1 |

These are retained client counters, not independently verified billing. The
client reported zero cost under the public subscription metadata; actual cost
remains unverified. No resource limit was hit, no source tool was used, and no
`fr` adoption or efficiency result follows from these attempts.

The next question is whether explicit lower reasoning effort permits a complete
submission within the same 2,048-token output allowance. Verify the provider
setting, freeze a new plan and retain its result before accepting the task.
The four code-change pilot attempts remain unstarted.
