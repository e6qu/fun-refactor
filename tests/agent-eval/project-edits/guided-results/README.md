# M2: four code changes with the same agents

GitHub independently checked all 48 scheduled local CLI attempts: 40 passed and 8 failed.
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
| Codex Luna Fast | ordinary | 8 / 8 | 26.1 | 208.7 | 26.1 |
| Codex Luna Fast | ordinary + fr | 8 / 8 | 32.9 | 263.3 | 32.9 |
| OpenCode2 GLM Flash | ordinary | 6 / 8 | 66.5 | 700.2 | 116.7 |
| OpenCode2 GLM Flash | ordinary + fr | 6 / 8 | 110.9 | 966.5 | 161.1 |
| OpenCode2 Kimi K3 | ordinary | 6 / 8 | 73.4 | 580.4 | 96.7 |
| OpenCode2 Kimi K3 | ordinary + fr | 6 / 8 | 90.5 | 814.9 | 135.8 |

Seconds spent per success includes all eight attempts, including failures.
The next table compares only the same requests and repetitions where both setups passed.

| Agent | Matching successful pairs | Ordinary mean seconds | With fr mean seconds | Observed time change |
| --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | 8 | 26.1 | 32.9 | +26.2% |
| OpenCode2 GLM Flash | 6 | 66.5 | 110.9 | +66.8% |
| OpenCode2 Kimi K3 | 6 | 73.4 | 90.5 | +23.3% |

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
| OpenCode2 GLM Flash | Add the escaped-text byte count | 0 | 0 |
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
| Codex Luna Fast | ordinary | 126661.4 | 105776.0 | 1417.6 | 475.9 |
| Codex Luna Fast | ordinary + fr | 175667.6 | 156560.0 | 2251.9 | 888.2 |
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
| Codex Luna Fast | Remove an unused Rust parameter | ordinary | 1 | passed | 23.239 | 0 | [patch](patches/codex-luna-fast-rust-parameter-files-1.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 29.037 | 3 | [patch](patches/codex-luna-fast-rust-parameter-fr-1.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary + fr | 1 | passed | 28.528 | 5 | [patch](patches/codex-luna-fast-typescript-alias-fr-1.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary | 1 | passed | 17.320 | 0 | [patch](patches/codex-luna-fast-typescript-alias-files-1.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary | 1 | passed | 43.460 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-files-1.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary + fr | 1 | passed | 46.913 | 2 | [patch](patches/codex-luna-fast-regex-escape-len-fr-1.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary + fr | 1 | passed | 30.404 | 5 | [patch](patches/codex-luna-fast-python-invoice-fr-1.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary | 1 | passed | 14.766 | 0 | [patch](patches/codex-luna-fast-python-invoice-files-1.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 124.252 | 4 | [patch](patches/opencode2-glm-flash-rust-parameter-fr-1.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary | 1 | passed | 96.989 | 0 | [patch](patches/opencode2-glm-flash-rust-parameter-files-1.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary | 1 | passed | 62.359 | 0 | [patch](patches/opencode2-glm-flash-typescript-alias-files-1.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary + fr | 1 | passed | 114.045 | 5 | [patch](patches/opencode2-glm-flash-typescript-alias-fr-1.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary + fr | 1 | time limit | 150.478 | 1 | none |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary | 1 | time limit | 150.520 | 0 | none |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary | 1 | passed | 50.933 | 0 | [patch](patches/opencode2-glm-flash-python-invoice-files-1.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary + fr | 1 | passed | 120.276 | 6 | [patch](patches/opencode2-glm-flash-python-invoice-fr-1.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary | 1 | passed | 118.834 | 0 | [patch](patches/opencode2-kimi-k3-rust-parameter-files-1.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 117.567 | 1 | [patch](patches/opencode2-kimi-k3-rust-parameter-fr-1.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary + fr | 1 | passed | 108.826 | 1 | [patch](patches/opencode2-kimi-k3-typescript-alias-fr-1.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary | 1 | passed | 64.310 | 0 | [patch](patches/opencode2-kimi-k3-typescript-alias-files-1.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary | 1 | failed | 78.121 | 0 | [patch](patches/opencode2-kimi-k3-regex-escape-len-files-1.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary + fr | 1 | failed | 121.568 | 5 | [patch](patches/opencode2-kimi-k3-regex-escape-len-fr-1.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary + fr | 1 | passed | 53.720 | 1 | [patch](patches/opencode2-kimi-k3-python-invoice-fr-1.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary | 1 | passed | 59.778 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-files-1.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 39.346 | 3 | [patch](patches/codex-luna-fast-rust-parameter-fr-2.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary | 2 | passed | 27.717 | 0 | [patch](patches/codex-luna-fast-rust-parameter-files-2.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary | 2 | passed | 20.708 | 0 | [patch](patches/codex-luna-fast-typescript-alias-files-2.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary + fr | 2 | passed | 23.639 | 3 | [patch](patches/codex-luna-fast-typescript-alias-fr-2.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary + fr | 2 | passed | 46.762 | 1 | [patch](patches/codex-luna-fast-regex-escape-len-fr-2.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary | 2 | passed | 45.254 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-files-2.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary | 2 | passed | 16.229 | 0 | [patch](patches/codex-luna-fast-python-invoice-files-2.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary + fr | 2 | passed | 18.719 | 1 | [patch](patches/codex-luna-fast-python-invoice-fr-2.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary | 2 | passed | 100.344 | 0 | [patch](patches/opencode2-glm-flash-rust-parameter-files-2.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 108.091 | 5 | [patch](patches/opencode2-glm-flash-rust-parameter-fr-2.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary + fr | 2 | passed | 99.848 | 5 | [patch](patches/opencode2-glm-flash-typescript-alias-fr-2.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary | 2 | passed | 49.940 | 0 | [patch](patches/opencode2-glm-flash-typescript-alias-files-2.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary | 2 | time limit | 150.523 | 0 | none |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary + fr | 2 | time limit | 150.447 | 7 | none |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary + fr | 2 | passed | 99.055 | 5 | [patch](patches/opencode2-glm-flash-python-invoice-fr-2.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary | 2 | passed | 38.570 | 0 | [patch](patches/opencode2-glm-flash-python-invoice-files-2.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 96.080 | 1 | [patch](patches/opencode2-kimi-k3-rust-parameter-fr-2.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary | 2 | passed | 123.960 | 0 | [patch](patches/opencode2-kimi-k3-rust-parameter-files-2.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary | 2 | passed | 37.878 | 0 | [patch](patches/opencode2-kimi-k3-typescript-alias-files-2.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary + fr | 2 | passed | 134.426 | 4 | [patch](patches/opencode2-kimi-k3-typescript-alias-fr-2.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary + fr | 2 | time limit; patch passed | 150.463 | 3 | [patch](patches/opencode2-kimi-k3-regex-escape-len-fr-2.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary | 2 | failed | 61.938 | 0 | [patch](patches/opencode2-kimi-k3-regex-escape-len-files-2.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary | 2 | passed | 35.631 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-files-2.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary + fr | 2 | passed | 32.217 | 1 | [patch](patches/opencode2-kimi-k3-python-invoice-fr-2.patch) |

## Milestone requirement

Every profile needs an accepted fr result for every request, with reviewed contributing fr actions.
That requirement remains open for:

- OpenCode2 GLM Flash: Add the escaped-text byte count.
- OpenCode2 Kimi K3: Add the escaped-text byte count.

Exact settings, versions, control runs, timing, token categories and original local record hashes are in [summary.json](summary.json).
