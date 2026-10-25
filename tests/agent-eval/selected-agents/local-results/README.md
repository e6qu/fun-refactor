# M1: the same agents with and without fr

All 36 scheduled local CLI attempts have independently checked outcomes: 27 passed and 9 failed.
Each agent received the same three requests twice with ordinary tools and twice with ordinary tools plus fr:

- Rename Boltons' `clamp` to `clamp_value`, including its declared callers and documentation. Limiting 70 to 0 through 50 must still return 50.
- Repair a Unicode text comparison. Comparing `éa` with `éab` must return `2/3`, counting adjacent character values rather than UTF-8 bytes.
- Write a Lean proof that the supplied access model returns true exactly for staff or owners. Keep the definitions and claim unchanged.

These are familiar development tasks. Two repetitions per case cannot establish a general success rate or general savings.
The baseline is the same app/model using its ordinary tools. Ordinary tools remain available when fr is added.

## Correct results and complete attempt time

A pass requires the CLI to finish within its frozen limits and its submitted patch to pass independent GitHub checks.
The checker applies the patch to the original project, checks the requested behavior or proof, checks delivery to another checkout,
and verifies exact undo and redo. Known incorrect and incomplete changes must fail the same checks.

GitHub independently checks patches. Timing, token and fr-use data are locally collected summaries; full transcripts stay local.

| Agent | Tools | Passed / attempted | Mean successful seconds | All attempt seconds | Seconds spent per success |
| --- | --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | ordinary | 6 / 6 | 31.7 | 190.4 | 31.7 |
| Codex Luna Fast | ordinary + fr | 6 / 6 | 39.1 | 234.8 | 39.1 |
| OpenCode2 GLM Flash | ordinary | 3 / 6 | 120.7 | 814.8 | 271.6 |
| OpenCode2 GLM Flash | ordinary + fr | 1 / 6 | 113.8 | 868.4 | 868.4 |
| OpenCode2 Kimi K3 | ordinary | 6 / 6 | 92.3 | 554.0 | 92.3 |
| OpenCode2 Kimi K3 | ordinary + fr | 5 / 6 | 97.8 | 640.1 | 128.0 |

Seconds spent per success divides time for **all six attempts**, including failures, by the number accepted.
Different success counts can cover different tasks. Use the next table for matching successful requests and repetitions.

| Agent | Pairs where both passed | Ordinary mean seconds | With fr mean seconds | Observed time change |
| --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | 6 | 31.7 | 39.1 | +23.3% |
| OpenCode2 GLM Flash | 1 | 111.3 | 113.8 | +2.2% |
| OpenCode2 Kimi K3 | 5 | 83.4 | 97.8 | +17.3% |

Positive time change means the fr setup took longer on those matching successes.
Recorded seconds start at controller attempt entry and include instructions, fresh task setup, indexing, CLI startup, model/tool/check work and export.
Controller Python startup and outer guard admission/startup are outside that timer and remain unmeasured.
They do not isolate fr execution time or prove why a model took longer. Shared environment preparation is separate in [summary.json](summary.json).

A token is a unit of text counted by a model. The next table uses those same matching successful pairs.
It shows means of each CLI's reported categories, rather than account charges. Their conventions differ;
cached or reasoning counts can overlap other columns, so do not add columns or compare totals across clients.
Usage from failed attempts remains in the summary and can be incomplete. No missing usage is replaced with zero.

| Agent | Tools | Mean reported input | Mean cached input | Mean reported output | Mean reasoning |
| --- | --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | ordinary | 126008.5 | 104448.0 | 2116.5 | 700.3 |
| Codex Luna Fast | ordinary + fr | 179054.8 | 155050.7 | 2819.2 | 1070.8 |
| OpenCode2 GLM Flash | ordinary | 15201.0 | 84992.0 | 1551.0 | 2047.0 |
| OpenCode2 GLM Flash | ordinary + fr | 15369.0 | 72320.0 | 572.0 | 961.0 |
| OpenCode2 Kimi K3 | ordinary | 10863.8 | 95897.6 | 1487.2 | 436.8 |
| OpenCode2 Kimi K3 | ordinary + fr | 10106.2 | 112076.8 | 1495.0 | 786.2 |

## Where the attempts succeeded

| Agent | Request | Ordinary passes / 2 | With fr passes / 2 |
| --- | --- | ---: | ---: |
| Codex Luna Fast | Python rename | 2 | 2 |
| Codex Luna Fast | Unicode repair | 2 | 2 |
| Codex Luna Fast | Access-rule proof | 2 | 2 |
| OpenCode2 GLM Flash | Python rename | 1 | 0 |
| OpenCode2 GLM Flash | Unicode repair | 0 | 0 |
| OpenCode2 GLM Flash | Access-rule proof | 2 | 1 |
| OpenCode2 Kimi K3 | Python rename | 2 | 2 |
| OpenCode2 Kimi K3 | Unicode repair | 2 | 1 |
| OpenCode2 Kimi K3 | Access-rule proof | 2 | 2 |

## Limits, costs and what was actually observed

Each attempt kept its original 150-second limit. The outer workstation guard stayed at 180 seconds. No failed slot was retried.
The first GLM/fr rename timed out after short tool steps; model replies used most of its time.
Its interrupted-session recorder incorrectly required a final outcome. That error remains in the original record and is fixed.
A second immutable execution packet continued only the unstarted slots, with the same prompts, settings, task inputs and limits.
Both packet hashes and original record hashes remain in the public summary.

Across the nine timeouts, recorded completed tools occupied 0.5 to 26.6 seconds of each 150-second task budget.
Overlapping tool intervals count once. Other elapsed time includes model replies, CLI overhead and any unrecorded unfinished call;
it is not a measurement of fr execution time. Per-attempt timing details remain in the summary.

A timed-out attempt can leave a patch. Its patch is checked and retained, but the attempt remains a failure.
An fr command count records use, not a benefit. Reading help alone does not show that fr contributed to a delivered change.
The two Codex Unicode attempts with fr available issued no observed fr command.

Actual account charges are unknown. A model catalog listing a zero price does not establish free billing.
The summary retains CLI-reported token categories separately by client, including cached tokens and incomplete usage after interruption.
No shared hard token/request cap was exposed by these CLIs. OpenCode2 also had its fixed 32-step limit.
Fresh conversations and task folders do not control operating-system or provider caches.
The clients record selected models; these records do not independently establish the provider backend's model identity.

Codex 0.164.0 selected Luna Fast with high reasoning. OpenCode2 2.0.32 selected GLM Flash or non-Pro Kimi K3 with high reasoning.
Local attempts used released fr 0.84.2, Rust 1.98.1 and Lean 4.28.0. Independent GitHub checking used Rust 1.99.1 and Lean 4.28.0.
The proof attempts started with the same generated model and unfinished claim. Proving that model does not establish the original source/model connection.

Only submitted patches and summaries are public. Full CLI logs, prompts, session exports, source archives and execution packets stay local.

## Every attempt

| Agent | Task | Tools | Repeat | Outcome | Local seconds | Fr commands | Patch |
| --- | --- | --- | ---: | --- | ---: | ---: | --- |
| Codex Luna Fast | Python rename | ordinary | 1 | passed | 32.814 | 0 | [patch](patches/codex-luna-fast-boltons-rename-files-1.patch) |
| Codex Luna Fast | Python rename | ordinary + fr | 1 | passed | 46.427 | 3 | [patch](patches/codex-luna-fast-boltons-rename-fr-1.patch) |
| Codex Luna Fast | Unicode repair | ordinary + fr | 1 | passed | 30.634 | 0 | [patch](patches/codex-luna-fast-unicode-dice-fr-1.patch) |
| Codex Luna Fast | Unicode repair | ordinary | 1 | passed | 32.464 | 0 | [patch](patches/codex-luna-fast-unicode-dice-files-1.patch) |
| Codex Luna Fast | Access-rule proof | ordinary | 1 | passed | 22.043 | 0 | [patch](patches/codex-luna-fast-access-proof-files-1.patch) |
| Codex Luna Fast | Access-rule proof | ordinary + fr | 1 | passed | 38.488 | 7 | [patch](patches/codex-luna-fast-access-proof-fr-1.patch) |
| OpenCode2 GLM Flash | Python rename | ordinary + fr | 1 | time limit | 151.019 | 8 | none |
| OpenCode2 GLM Flash | Python rename | ordinary | 1 | time limit | 150.957 | 0 | none |
| OpenCode2 GLM Flash | Unicode repair | ordinary | 1 | time limit | 150.962 | 0 | none |
| OpenCode2 GLM Flash | Unicode repair | ordinary + fr | 1 | time limit | 150.911 | 3 | none |
| OpenCode2 GLM Flash | Access-rule proof | ordinary + fr | 1 | passed | 113.755 | 3 | [patch](patches/opencode2-glm-flash-access-proof-fr-1.patch) |
| OpenCode2 GLM Flash | Access-rule proof | ordinary | 1 | passed | 111.324 | 0 | [patch](patches/opencode2-glm-flash-access-proof-files-1.patch) |
| OpenCode2 Kimi K3 | Python rename | ordinary | 1 | passed | 93.213 | 0 | [patch](patches/opencode2-kimi-k3-boltons-rename-files-1.patch) |
| OpenCode2 Kimi K3 | Python rename | ordinary + fr | 1 | passed | 135.421 | 4 | [patch](patches/opencode2-kimi-k3-boltons-rename-fr-1.patch) |
| OpenCode2 Kimi K3 | Unicode repair | ordinary + fr | 1 | time limit | 150.810 | 1 | none |
| OpenCode2 Kimi K3 | Unicode repair | ordinary | 1 | passed | 136.807 | 0 | [patch](patches/opencode2-kimi-k3-unicode-dice-files-1.patch) |
| OpenCode2 Kimi K3 | Access-rule proof | ordinary | 1 | passed | 38.047 | 0 | [patch](patches/opencode2-kimi-k3-access-proof-files-1.patch) |
| OpenCode2 Kimi K3 | Access-rule proof | ordinary + fr | 1 | passed | 59.309 | 2 | [patch](patches/opencode2-kimi-k3-access-proof-fr-1.patch) |
| Codex Luna Fast | Python rename | ordinary + fr | 2 | passed | 44.961 | 6 | [patch](patches/codex-luna-fast-boltons-rename-fr-2.patch) |
| Codex Luna Fast | Python rename | ordinary | 2 | passed | 34.062 | 0 | [patch](patches/codex-luna-fast-boltons-rename-files-2.patch) |
| Codex Luna Fast | Unicode repair | ordinary | 2 | passed | 35.571 | 0 | [patch](patches/codex-luna-fast-unicode-dice-files-2.patch) |
| Codex Luna Fast | Unicode repair | ordinary + fr | 2 | passed | 41.718 | 0 | [patch](patches/codex-luna-fast-unicode-dice-fr-2.patch) |
| Codex Luna Fast | Access-rule proof | ordinary + fr | 2 | passed | 32.619 | 1 | [patch](patches/codex-luna-fast-access-proof-fr-2.patch) |
| Codex Luna Fast | Access-rule proof | ordinary | 2 | passed | 33.439 | 0 | [patch](patches/codex-luna-fast-access-proof-files-2.patch) |
| OpenCode2 GLM Flash | Python rename | ordinary | 2 | passed | 123.745 | 0 | [patch](patches/opencode2-glm-flash-boltons-rename-files-2.patch) |
| OpenCode2 GLM Flash | Python rename | ordinary + fr | 2 | time limit | 150.911 | 8 | none |
| OpenCode2 GLM Flash | Unicode repair | ordinary + fr | 2 | time limit | 150.839 | 0 | none |
| OpenCode2 GLM Flash | Unicode repair | ordinary | 2 | time limit | 150.872 | 0 | none |
| OpenCode2 GLM Flash | Access-rule proof | ordinary | 2 | passed | 126.956 | 0 | [patch](patches/opencode2-glm-flash-access-proof-files-2.patch) |
| OpenCode2 GLM Flash | Access-rule proof | ordinary + fr | 2 | time limit; patch passed | 150.919 | 5 | [patch](patches/opencode2-glm-flash-access-proof-fr-2.patch) |
| OpenCode2 Kimi K3 | Python rename | ordinary + fr | 2 | passed | 108.848 | 7 | [patch](patches/opencode2-kimi-k3-boltons-rename-fr-2.patch) |
| OpenCode2 Kimi K3 | Python rename | ordinary | 2 | passed | 125.712 | 0 | [patch](patches/opencode2-kimi-k3-boltons-rename-files-2.patch) |
| OpenCode2 Kimi K3 | Unicode repair | ordinary | 2 | passed | 112.881 | 0 | [patch](patches/opencode2-kimi-k3-unicode-dice-files-2.patch) |
| OpenCode2 Kimi K3 | Unicode repair | ordinary + fr | 2 | passed | 134.662 | 1 | [patch](patches/opencode2-kimi-k3-unicode-dice-fr-2.patch) |
| OpenCode2 Kimi K3 | Access-rule proof | ordinary + fr | 2 | passed | 51.005 | 0 | [patch](patches/opencode2-kimi-k3-access-proof-fr-2.patch) |
| OpenCode2 Kimi K3 | Access-rule proof | ordinary | 2 | passed | 47.301 | 0 | [patch](patches/opencode2-kimi-k3-access-proof-files-2.patch) |

Independent check runs: [GitHub 40216479622](https://github.com/e6qu/fun-refactor/actions/runs/40216479622).
The full per-attempt settings, metrics, fr operations and original record hashes are in [summary.json](summary.json).
See [PLAN.md](../../../../PLAN.md#next-work) for the next complete milestone.
