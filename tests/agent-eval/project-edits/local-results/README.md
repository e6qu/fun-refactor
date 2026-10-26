# M2: four code changes with the same agents

GitHub independently checked all 48 scheduled local CLI attempts: 43 passed and 5 failed.
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
| Codex Luna Fast | ordinary | 8 / 8 | 29.4 | 234.8 | 29.4 |
| Codex Luna Fast | ordinary + fr | 8 / 8 | 39.0 | 312.2 | 39.0 |
| OpenCode2 GLM Flash | ordinary | 7 / 8 | 55.3 | 537.8 | 76.8 |
| OpenCode2 GLM Flash | ordinary + fr | 6 / 8 | 80.9 | 785.8 | 131.0 |
| OpenCode2 Kimi K3 | ordinary | 8 / 8 | 73.3 | 586.5 | 73.3 |
| OpenCode2 Kimi K3 | ordinary + fr | 6 / 8 | 81.4 | 788.8 | 131.5 |

Seconds spent per success includes all eight attempts, including failures.
The next table compares only the same requests and repetitions where both setups passed.

| Agent | Matching successful pairs | Ordinary mean seconds | With fr mean seconds | Observed time change |
| --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | 8 | 29.4 | 39.0 | +33.0% |
| OpenCode2 GLM Flash | 5 | 50.7 | 75.8 | +49.5% |
| OpenCode2 Kimi K3 | 6 | 69.7 | 81.4 | +16.8% |

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
| OpenCode2 GLM Flash | Add the escaped-text byte count | 1 | 1 |
| OpenCode2 GLM Flash | Include the invoice fee | 2 | 1 |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | 2 | 1 |
| OpenCode2 Kimi K3 | Rename a TypeScript export | 2 | 2 |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | 2 | 1 |
| OpenCode2 Kimi K3 | Include the invoice fee | 2 | 2 |

## Reported tokens and limits

A token is a unit of text counted by a model. Means below use the same matching successful pairs.
CLI categories differ and can overlap. Do not add columns or compare token totals across clients.
Missing usage and actual account charges remain unknown. A zero catalog price does not establish free billing.

| Agent | Tools | Mean reported input | Mean cached input | Mean reported output | Mean reasoning |
| --- | --- | ---: | ---: | ---: | ---: |
| Codex Luna Fast | ordinary | 130464.9 | 110064.0 | 1681.1 | 703.9 |
| Codex Luna Fast | ordinary + fr | 193249.0 | 163408.0 | 2590.5 | 1155.5 |
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
| Codex Luna Fast | Remove an unused Rust parameter | ordinary | 1 | passed | 32.203 | 0 | [patch](patches/codex-luna-fast-rust-parameter-files-1.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 34.271 | 1 | [patch](patches/codex-luna-fast-rust-parameter-fr-1.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary + fr | 1 | passed | 44.824 | 8 | [patch](patches/codex-luna-fast-typescript-alias-fr-1.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary | 1 | passed | 24.869 | 0 | [patch](patches/codex-luna-fast-typescript-alias-files-1.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary | 1 | passed | 39.723 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-files-1.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary + fr | 1 | passed | 42.491 | 1 | [patch](patches/codex-luna-fast-regex-escape-len-fr-1.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary + fr | 1 | passed | 44.267 | 7 | [patch](patches/codex-luna-fast-python-invoice-fr-1.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary | 1 | passed | 27.187 | 0 | [patch](patches/codex-luna-fast-python-invoice-files-1.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 72.283 | 1 | [patch](patches/opencode2-glm-flash-rust-parameter-fr-1.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary | 1 | passed | 52.864 | 0 | [patch](patches/opencode2-glm-flash-rust-parameter-files-1.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary | 1 | passed | 42.319 | 0 | [patch](patches/opencode2-glm-flash-typescript-alias-files-1.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary + fr | 1 | passed | 81.719 | 2 | [patch](patches/opencode2-glm-flash-typescript-alias-fr-1.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary + fr | 1 | time limit; patch passed | 150.486 | 1 | [patch](patches/opencode2-glm-flash-regex-escape-len-fr-1.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary | 1 | passed | 104.701 | 0 | [patch](patches/opencode2-glm-flash-regex-escape-len-files-1.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary | 1 | passed | 29.058 | 0 | [patch](patches/opencode2-glm-flash-python-invoice-files-1.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary + fr | 1 | time limit; patch passed | 150.081 | 9 | [patch](patches/opencode2-glm-flash-python-invoice-fr-1.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary | 1 | passed | 48.763 | 0 | [patch](patches/opencode2-kimi-k3-rust-parameter-files-1.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary + fr | 1 | passed | 113.044 | 6 | [patch](patches/opencode2-kimi-k3-rust-parameter-fr-1.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary + fr | 1 | passed | 56.009 | 1 | [patch](patches/opencode2-kimi-k3-typescript-alias-fr-1.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary | 1 | passed | 43.331 | 0 | [patch](patches/opencode2-kimi-k3-typescript-alias-files-1.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary | 1 | passed | 85.682 | 0 | [patch](patches/opencode2-kimi-k3-regex-escape-len-files-1.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary + fr | 1 | time limit; patch passed | 150.475 | 3 | [patch](patches/opencode2-kimi-k3-regex-escape-len-fr-1.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary + fr | 1 | passed | 90.247 | 1 | [patch](patches/opencode2-kimi-k3-python-invoice-fr-1.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary | 1 | passed | 91.897 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-files-1.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 47.778 | 8 | [patch](patches/codex-luna-fast-rust-parameter-fr-2.patch) |
| Codex Luna Fast | Remove an unused Rust parameter | ordinary | 2 | passed | 29.586 | 0 | [patch](patches/codex-luna-fast-rust-parameter-files-2.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary | 2 | passed | 22.377 | 0 | [patch](patches/codex-luna-fast-typescript-alias-files-2.patch) |
| Codex Luna Fast | Rename a TypeScript export | ordinary + fr | 2 | passed | 32.209 | 6 | [patch](patches/codex-luna-fast-typescript-alias-fr-2.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary + fr | 2 | passed | 45.034 | 3 | [patch](patches/codex-luna-fast-regex-escape-len-fr-2.patch) |
| Codex Luna Fast | Add the escaped-text byte count | ordinary | 2 | passed | 40.845 | 0 | [patch](patches/codex-luna-fast-regex-escape-len-files-2.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary | 2 | passed | 18.022 | 0 | [patch](patches/codex-luna-fast-python-invoice-files-2.patch) |
| Codex Luna Fast | Include the invoice fee | ordinary + fr | 2 | passed | 21.309 | 1 | [patch](patches/codex-luna-fast-python-invoice-fr-2.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary | 2 | passed | 82.740 | 0 | [patch](patches/opencode2-glm-flash-rust-parameter-files-2.patch) |
| OpenCode2 GLM Flash | Remove an unused Rust parameter | ordinary + fr | 2 | passed | 117.454 | 5 | [patch](patches/opencode2-glm-flash-rust-parameter-fr-2.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary + fr | 2 | passed | 71.135 | 1 | [patch](patches/opencode2-glm-flash-typescript-alias-fr-2.patch) |
| OpenCode2 GLM Flash | Rename a TypeScript export | ordinary | 2 | passed | 43.320 | 0 | [patch](patches/opencode2-glm-flash-typescript-alias-files-2.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary | 2 | time limit; patch passed | 150.474 | 0 | [patch](patches/opencode2-glm-flash-regex-escape-len-files-2.patch) |
| OpenCode2 GLM Flash | Add the escaped-text byte count | ordinary + fr | 2 | passed | 106.131 | 1 | [patch](patches/opencode2-glm-flash-regex-escape-len-fr-2.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary + fr | 2 | passed | 36.556 | 1 | [patch](patches/opencode2-glm-flash-python-invoice-fr-2.patch) |
| OpenCode2 GLM Flash | Include the invoice fee | ordinary | 2 | passed | 32.303 | 0 | [patch](patches/opencode2-glm-flash-python-invoice-files-2.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary + fr | 2 | time limit; patch passed | 150.060 | 7 | [patch](patches/opencode2-kimi-k3-rust-parameter-fr-2.patch) |
| OpenCode2 Kimi K3 | Remove an unused Rust parameter | ordinary | 2 | passed | 82.607 | 0 | [patch](patches/opencode2-kimi-k3-rust-parameter-files-2.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary | 2 | passed | 59.823 | 0 | [patch](patches/opencode2-kimi-k3-typescript-alias-files-2.patch) |
| OpenCode2 Kimi K3 | Rename a TypeScript export | ordinary + fr | 2 | passed | 63.763 | 1 | [patch](patches/opencode2-kimi-k3-typescript-alias-fr-2.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary + fr | 2 | passed | 122.721 | 1 | [patch](patches/opencode2-kimi-k3-regex-escape-len-fr-2.patch) |
| OpenCode2 Kimi K3 | Add the escaped-text byte count | ordinary | 2 | passed | 137.572 | 0 | [patch](patches/opencode2-kimi-k3-regex-escape-len-files-2.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary | 2 | passed | 36.836 | 0 | [patch](patches/opencode2-kimi-k3-python-invoice-files-2.patch) |
| OpenCode2 Kimi K3 | Include the invoice fee | ordinary + fr | 2 | passed | 42.503 | 1 | [patch](patches/opencode2-kimi-k3-python-invoice-fr-2.patch) |

## Milestone requirement

Every profile needs an accepted fr result for every request, with reviewed contributing fr actions.
That requirement passed.


Exact settings, versions, control runs, timing, token categories and original local record hashes are in [summary.json](summary.json).
