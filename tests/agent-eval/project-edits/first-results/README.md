# M2: four code changes with the same agents

GitHub independently checked all 48 scheduled local CLI attempts: 33 passed and 15 failed.
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
| Codex Luna Fast | ordinary | 8 / 8 | 29.9 | 239.1 | 29.9 |
| Codex Luna Fast | ordinary + fr | 8 / 8 | 39.8 | 318.8 | 39.8 |
| OpenCode2 GLM Flash | ordinary | 5 / 8 | 84.9 | 875.4 | 175.1 |
| OpenCode2 GLM Flash | ordinary + fr | 0 / 8 | undefined | 1201.3 | undefined |
| OpenCode2 Kimi K3 | ordinary | 6 / 8 | 55.6 | 634.4 | 105.7 |
| OpenCode2 Kimi K3 | ordinary + fr | 6 / 8 | 96.6 | 880.9 | 146.8 |

Seconds spent per success includes all eight attempts, including failures.
The next table compares only the same requests and repetitions where both setups passed.

| Agent | Matching successful pairs | Ordinary mean seconds | With fr mean seconds | Observed time change |
| --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | 8 | 29.9 | 39.8 | +33.3% |
| OpenCode2 GLM Flash | 0 | undefined | undefined | undefined |
| OpenCode2 Kimi K3 | 6 | 55.6 | 96.6 | +73.9% |

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
| OpenCode2 GLM Flash | Remove an unused Rust parameter | 1 | 0 |
| OpenCode2 GLM Flash | Rename a TypeScript export | 2 | 0 |
| OpenCode2 GLM Flash | Add the escaped-text byte count | 0 | 0 |
| OpenCode2 GLM Flash | Include the invoice fee | 2 | 0 |
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
| Codex Luna Fast | ordinary | 154732.2 | 135744.0 | 2150.9 | 873.9 |
| Codex Luna Fast | ordinary + fr | 222447.1 | 199488.0 | 2755.4 | 1234.0 |
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
| Codex Luna Fast | Remove an unused Rust parameter | ordinary | 1 | passed | 25.110 | 0 | [patch](patches/codex-luna-fast-rust-parameter-files-1.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 35.947 | 4 | [patch](patches/codex-luna-fast-rust-parameter-fr-1.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary + fr | 1 | passed | 31.102 | 4 | [patch](patches/codex-luna-fast-typescript-alias-fr-1.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary | 1 | passed | 23.762 | 0 | [patch](patches/codex-luna-fast-typescript-alias-files-1.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary | 1 | passed | 49.645 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-files-1.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary + fr | 1 | passed | 62.352 | 3 | [patch](patches/codex-luna-fast-regex-escape-len-fr-1.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary + fr | 1 | passed | 19.020 | 0 | [patch](patches/codex-luna-fast-python-invoice-fr-1.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary | 1 | passed | 16.949 | 0 | [patch](patches/codex-luna-fast-python-invoice-files-1.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary + fr | 1 | time limit | 150.072 | 5 | none |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary | 1 | passed | 108.922 | 0 | [patch](patches/opencode2-glm-flash-rust-parameter-files-1.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary | 1 | passed | 80.497 | 0 | [patch](patches/opencode2-glm-flash-typescript-alias-files-1.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary + fr | 1 | time limit | 150.034 | 7 | none |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary + fr | 1 | time limit | 150.483 | 0 | none |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary | 1 | time limit | 150.426 | 0 | none |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary | 1 | passed | 64.966 | 0 | [patch](patches/opencode2-glm-flash-python-invoice-files-1.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary + fr | 1 | time limit | 150.015 | 5 | none |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary | 1 | passed | 57.693 | 0 | [patch](patches/opencode2-kimi-k3-rust-parameter-files-1.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 99.887 | 1 | [patch](patches/opencode2-kimi-k3-rust-parameter-fr-1.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary + fr | 1 | passed | 90.327 | 6 | [patch](patches/opencode2-kimi-k3-typescript-alias-fr-1.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary | 1 | passed | 57.526 | 0 | [patch](patches/opencode2-kimi-k3-typescript-alias-files-1.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary | 1 | time limit | 150.468 | 0 | none |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary + fr | 1 | time limit | 150.441 | 0 | none |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary + fr | 1 | passed | 42.151 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-fr-1.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary | 1 | passed | 34.013 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-files-1.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 38.475 | 6 | [patch](patches/codex-luna-fast-rust-parameter-fr-2.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary | 2 | passed | 28.373 | 0 | [patch](patches/codex-luna-fast-rust-parameter-files-2.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary | 2 | passed | 20.210 | 0 | [patch](patches/codex-luna-fast-typescript-alias-files-2.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary + fr | 2 | passed | 33.195 | 5 | [patch](patches/codex-luna-fast-typescript-alias-fr-2.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary + fr | 2 | passed | 60.233 | 1 | [patch](patches/codex-luna-fast-regex-escape-len-fr-2.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary | 2 | passed | 59.219 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-files-2.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary | 2 | passed | 15.844 | 0 | [patch](patches/codex-luna-fast-python-invoice-files-2.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary + fr | 2 | passed | 38.454 | 4 | [patch](patches/codex-luna-fast-python-invoice-fr-2.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary | 2 | time limit | 150.049 | 0 | none |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary + fr | 2 | time limit | 150.036 | 6 | none |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary + fr | 2 | time limit | 150.066 | 4 | none |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary | 2 | passed | 99.368 | 0 | [patch](patches/opencode2-glm-flash-typescript-alias-files-2.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary | 2 | time limit | 150.523 | 0 | none |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary + fr | 2 | time limit | 150.527 | 0 | none |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary + fr | 2 | time limit; patch passed | 150.055 | 6 | [patch](patches/opencode2-glm-flash-python-invoice-fr-2.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary | 2 | passed | 70.632 | 0 | [patch](patches/opencode2-glm-flash-python-invoice-files-2.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 128.844 | 4 | [patch](patches/opencode2-kimi-k3-rust-parameter-fr-2.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary | 2 | passed | 50.582 | 0 | [patch](patches/opencode2-kimi-k3-rust-parameter-files-2.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary | 2 | passed | 74.113 | 0 | [patch](patches/opencode2-kimi-k3-typescript-alias-files-2.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary + fr | 2 | passed | 101.059 | 6 | [patch](patches/opencode2-kimi-k3-typescript-alias-fr-2.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary + fr | 2 | time limit | 150.565 | 1 | none |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary | 2 | time limit | 150.572 | 0 | none |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary | 2 | passed | 59.452 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-files-2.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary + fr | 2 | passed | 117.579 | 6 | [patch](patches/opencode2-kimi-k3-python-invoice-fr-2.patch) |

## Milestone requirement

Every profile needs an accepted fr result for every request, with reviewed contributing fr actions.
That requirement remains open for:

- OpenCode2 GLM Flash: Remove an unused Rust parameter.
- OpenCode2 GLM Flash: Rename a TypeScript export.
- OpenCode2 GLM Flash: Add the escaped-text byte count.
- OpenCode2 GLM Flash: Include the invoice fee.
- OpenCode2 Kimi K3: Add the escaped-text byte count.

Exact settings, versions, control runs, timing, token categories and original local record hashes are in [summary.json](summary.json).
