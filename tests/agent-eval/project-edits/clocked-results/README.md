# M2: four code changes with the same agents

GitHub independently checked all 48 scheduled local CLI attempts: 41 passed and 7 failed.
The baseline is the same app and model using ordinary reading, search, editing and shell tools.
The other setup adds fr and its project skill. Each setup gets two attempts at each request:

- Remove an unused true-or-false input, called a parameter, across two Rust packages. Update all five calls.
- In `import { price as displayPrice }`, rename the exported `price` to `quote`. Keep local calls named `displayPrice` and the unrelated `price` function.
- Count escaped-text bytes in both regex packages. Escaping adds backslashes: `a.b` needs four bytes; `é` uses two UTF-8 bytes. An allocation reserves memory; counting must allocate nothing.
- Include the invoice fee: `invoice_total(5, 2)` must return 7.

These are familiar development tasks. Two repetitions cannot establish general savings or a general success rate.
An accepted result must finish within the frozen limits and deliver a patch that passes the independent checks.
The checker applies each patch to the original source, runs compiler and behavior checks, and verifies exact undo and redo.
Timeouts stay failed even if their patches pass. Supplied controls are checked separately and are not agent successes.

## Results and complete attempt time

| Agent | Tools | Passed / attempted | Mean successful seconds | All attempt seconds | Seconds spent per success |
| --- | --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | ordinary | 8 / 8 | 32.9 | 263.2 | 32.9 |
| Codex Luna Fast | ordinary + fr | 8 / 8 | 53.4 | 426.8 | 53.4 |
| OpenCode2 GLM Flash | ordinary | 7 / 8 | 65.8 | 597.7 | 85.4 |
| OpenCode2 GLM Flash | ordinary + fr | 6 / 8 | 71.0 | 726.9 | 121.2 |
| OpenCode2 Kimi K3 | ordinary | 6 / 8 | 38.5 | 494.2 | 82.4 |
| OpenCode2 Kimi K3 | ordinary + fr | 6 / 8 | 73.7 | 728.6 | 121.4 |

Seconds spent per success includes all eight attempts, including failures.
The next table compares only the same requests and repetitions where both setups passed.

| Agent | Matching successful pairs | Ordinary mean seconds | With fr mean seconds | Observed time change |
| --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | 8 | 32.9 | 53.4 | +62.2% |
| OpenCode2 GLM Flash | 6 | 53.9 | 71.0 | +31.7% |
| OpenCode2 Kimi K3 | 6 | 38.5 | 73.7 | +91.3% |

Positive time change means that adding fr took longer on those matching successes.
These are complete code-change times, not execution-speed benchmarks of the changed programs.
Preparation, instructions, indexing, model replies, tools, checks and result export share the 150-second deadline.
Controller Python startup and outer guard startup remain unmeasured; shared preparation is separate in [summary.json](summary.json).
CPU and memory samples cover the native process group. The outer workstation guard also covers controller preparation and recording.
A command count alone establishes no benefit. Each contribution review compares commands and results with the delivered patch.

## Requests completed

| Agent | Request | Ordinary passes / 2 | With fr passes / 2 |
| --- | --- | ---: | ---: |
| Codex Luna Fast | Remove an unused Rust parameter | 2 | 2 |
| Codex Luna Fast | Rename a TypeScript export | 2 | 2 |
| Codex Luna Fast | Add the escaped-text byte count | 2 | 2 |
| Codex Luna Fast | Include the invoice fee | 2 | 2 |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | 2 | 2 |
| OpenCode2 GLM Flash | Rename a TypeScript export | 2 | 2 |
| OpenCode2 GLM Flash | Add the escaped-text byte count | 1 | 0 |
| OpenCode2 GLM Flash | Include the invoice fee | 2 | 2 |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | 2 | 2 |
| OpenCode2 Kimi K3 | Rename a TypeScript export | 2 | 2 |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | 0 | 0 |
| OpenCode2 Kimi K3 | Include the invoice fee | 2 | 2 |

## Reported tokens and limits

A token is a unit of text counted by a model. Means below use the same matching successful pairs.
CLI categories differ and can overlap. Do not add columns or compare token totals across clients.
Missing usage and actual account charges remain unknown. A zero catalog price does not establish free billing.

| Agent | Tools | Mean reported input | Mean cached input | Mean reported output | Mean reasoning |
| --- | --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | ordinary | 127056.0 | 106240.0 | 2072.5 | 1104.2 |
| Codex Luna Fast | ordinary + fr | 304549.8 | 271744.0 | 2830.1 | 1240.9 |
| OpenCode2 GLM Flash | ordinary | unknown | unknown | unknown | unknown |
| OpenCode2 GLM Flash | ordinary + fr | unknown | unknown | unknown | unknown |
| OpenCode2 Kimi K3 | ordinary | unknown | unknown | unknown | unknown |
| OpenCode2 Kimi K3 | ordinary + fr | unknown | unknown | unknown | unknown |

All attempts ran serially with the same 150-second deadline. No failed slot was retried.
The outer workstation guard kept its 180-second deadline and resource limits.
The native CLIs expose no shared hard token/request cap. Fresh conversations do not control operating-system or provider caches.
The clients record their selected models. These records cannot independently verify which model a provider ran or the account charge.
Full CLI logs, prompts, session exports, source archives and execution packets stay local. We publish only patches and summaries.

## Every attempt

| Agent | Request | Tools | Repeat | Outcome | Local seconds | Fr commands | Patch |
| --- | --- | --- | ---: | --- | ---: | ---: | --- |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary | 1 | passed | 27.898 | 0 | [patch](patches/codex-luna-fast-rust-parameter-files-1.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 47.603 | 8 | [patch](patches/codex-luna-fast-rust-parameter-fr-1.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary + fr | 1 | passed | 35.992 | 0 | [patch](patches/codex-luna-fast-typescript-alias-fr-1.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary | 1 | passed | 22.735 | 0 | [patch](patches/codex-luna-fast-typescript-alias-files-1.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary | 1 | passed | 65.715 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-files-1.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary + fr | 1 | passed | 102.104 | 3 | [patch](patches/codex-luna-fast-regex-escape-len-fr-1.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary + fr | 1 | passed | 18.654 | 0 | [patch](patches/codex-luna-fast-python-invoice-fr-1.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary | 1 | passed | 22.625 | 0 | [patch](patches/codex-luna-fast-python-invoice-files-1.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 98.820 | 5 | [patch](patches/opencode2-glm-flash-rust-parameter-fr-1.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary | 1 | passed | 96.336 | 0 | [patch](patches/opencode2-glm-flash-rust-parameter-files-1.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary | 1 | passed | 48.375 | 0 | [patch](patches/opencode2-glm-flash-typescript-alias-files-1.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary + fr | 1 | passed | 59.859 | 1 | [patch](patches/opencode2-glm-flash-typescript-alias-fr-1.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary + fr | 1 | time limit; patch passed | 150.541 | 1 | [patch](patches/opencode2-glm-flash-regex-escape-len-fr-1.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary | 1 | failed | 137.027 | 0 | [patch](patches/opencode2-glm-flash-regex-escape-len-files-1.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary | 1 | passed | 32.235 | 0 | [patch](patches/opencode2-glm-flash-python-invoice-files-1.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary + fr | 1 | passed | 62.802 | 5 | [patch](patches/opencode2-glm-flash-python-invoice-fr-1.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary | 1 | passed | 58.485 | 0 | [patch](patches/opencode2-kimi-k3-rust-parameter-files-1.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 61.224 | 1 | [patch](patches/opencode2-kimi-k3-rust-parameter-fr-1.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary + fr | 1 | passed | 61.216 | 1 | [patch](patches/opencode2-kimi-k3-typescript-alias-fr-1.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary | 1 | passed | 30.075 | 0 | [patch](patches/opencode2-kimi-k3-typescript-alias-files-1.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary | 1 | time limit | 150.521 | 0 | [patch](patches/opencode2-kimi-k3-regex-escape-len-files-1.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary + fr | 1 | time limit | 150.526 | 2 | [patch](patches/opencode2-kimi-k3-regex-escape-len-fr-1.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary + fr | 1 | passed | 132.853 | 5 | [patch](patches/opencode2-kimi-k3-python-invoice-fr-1.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary | 1 | passed | 29.553 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-files-1.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 45.466 | 8 | [patch](patches/codex-luna-fast-rust-parameter-fr-2.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary | 2 | passed | 30.744 | 0 | [patch](patches/codex-luna-fast-rust-parameter-files-2.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary | 2 | passed | 21.406 | 0 | [patch](patches/codex-luna-fast-typescript-alias-files-2.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary + fr | 2 | passed | 44.579 | 11 | [patch](patches/codex-luna-fast-typescript-alias-fr-2.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary + fr | 2 | passed | 102.526 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-fr-2.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary | 2 | passed | 56.406 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-files-2.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary | 2 | passed | 15.709 | 0 | [patch](patches/codex-luna-fast-python-invoice-files-2.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary + fr | 2 | passed | 29.919 | 7 | [patch](patches/codex-luna-fast-python-invoice-fr-2.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary | 2 | passed | 56.979 | 0 | [patch](patches/opencode2-glm-flash-rust-parameter-files-2.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 77.013 | 7 | [patch](patches/opencode2-glm-flash-rust-parameter-fr-2.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary + fr | 2 | passed | 73.439 | 5 | [patch](patches/opencode2-glm-flash-typescript-alias-fr-2.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary | 2 | passed | 51.671 | 0 | [patch](patches/opencode2-glm-flash-typescript-alias-files-2.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary | 2 | passed | 137.231 | 0 | [patch](patches/opencode2-glm-flash-regex-escape-len-files-2.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary + fr | 2 | time limit; patch passed | 150.558 | 2 | [patch](patches/opencode2-glm-flash-regex-escape-len-fr-2.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary + fr | 2 | passed | 53.907 | 1 | [patch](patches/opencode2-glm-flash-python-invoice-fr-2.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary | 2 | passed | 37.820 | 0 | [patch](patches/opencode2-glm-flash-python-invoice-files-2.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 49.590 | 1 | [patch](patches/opencode2-kimi-k3-rust-parameter-fr-2.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary | 2 | passed | 51.212 | 0 | [patch](patches/opencode2-kimi-k3-rust-parameter-files-2.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary | 2 | passed | 34.457 | 0 | [patch](patches/opencode2-kimi-k3-typescript-alias-files-2.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary + fr | 2 | passed | 73.387 | 3 | [patch](patches/opencode2-kimi-k3-typescript-alias-fr-2.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary + fr | 2 | failed | 136.075 | 1 | [patch](patches/opencode2-kimi-k3-regex-escape-len-fr-2.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary | 2 | failed | 112.695 | 0 | [patch](patches/opencode2-kimi-k3-regex-escape-len-files-2.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary | 2 | passed | 27.239 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-files-2.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary + fr | 2 | passed | 63.735 | 1 | [patch](patches/opencode2-kimi-k3-python-invoice-fr-2.patch) |

## Milestone requirement

Every profile needs an accepted fr result for every request, with reviewed contributing fr actions.
That requirement remains open for:

- OpenCode2 GLM Flash: Add the escaped-text byte count.
- OpenCode2 Kimi K3: Add the escaped-text byte count.

Exact settings, versions, control runs, timing, token categories and original local record hashes are in [summary.json](summary.json).
