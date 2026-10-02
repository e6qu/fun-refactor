# Native code-change outcomes and observed costs

These reviewed integration tasks establish no general efficiency advantage.
Failed attempts remain in cost totals. Missing totals are unknown, not zero.

## 2026-10-02-code-changes

Plan: `76243d6223c4b764fb85a16d2437715b2b611a45e211f350af5562f9046c6217`.

| Configured model | Tools | Outcome | Host calls | fr calls | Result bytes | Collection seconds | CLI steps complete |
|---|---|---|---:|---:|---:|---:|---|
| zai-coding-plan/glm-5.3-flash | fr | failed | 8 | 0 | 19095 | 120.1 | no |
| zai-coding-plan/glm-5.3-flash | files | passed | 11 | 0 | 19536 | 91.5 | yes |
| kimi-code-plan-global/k3 | files | passed | 4 | 0 | 10509 | 64.4 | yes |
| kimi-code-plan-global/k3 | fr | passed | 5 | 0 | 10741 | 53.0 | yes |
| kimi-code-plan-global/k3 | files | passed | 6 | 0 | 11912 | 34.5 | yes |
| kimi-code-plan-global/k3 | fr | passed | 10 | 0 | 26438 | 91.8 | yes |
| zai-coding-plan/glm-5.3-flash | files | passed | 11 | 0 | 10738 | 105.5 | yes |
| zai-coding-plan/glm-5.3-flash | fr | passed | 13 | 0 | 18733 | 107.3 | yes |

| Configured model | Tools | Outcomes | Collection seconds per behavior pass |
|---|---|---|---:|
| kimi-code-plan-global/k3 | files | 2 passed | 49.5 |
| kimi-code-plan-global/k3 | fr | 2 passed | 72.4 |
| zai-coding-plan/glm-5.3-flash | files | 2 passed | 98.5 |
| zai-coding-plan/glm-5.3-flash | fr | 1 failed, 1 passed | 227.4 |

Collection time includes failed attempts; it excludes remote grading time.
Host bytes are produced results, not complete model context. See JSON for native stream confirmations,
partial usage, resource samples, tool schemas and paired outcomes. Provider billing remains unverified.

## 2026-10-02-public-edit-code-changes

Plan: `5217e3521e71dc2b78de5bd787b6631db8c321f83cca13cce05658a3a4f699dd`.

| Configured model | Tools | Outcome | Host calls | fr calls | Result bytes | Collection seconds | CLI steps complete |
|---|---|---|---:|---:|---:|---:|---|
| kimi-code-plan-global/k3 | fr | pending_grading | 6 | 0 | 3281 | 65.7 | yes |
| kimi-code-plan-global/k3 | files | pending_grading | 7 | 0 | 16052 | 59.2 | yes |
| zai-coding-plan/glm-5.3-flash | fr | pending_grading | 10 | 0 | 5626 | 100.6 | yes |
| zai-coding-plan/glm-5.3-flash | files | failed | 12 | 0 | 33864 | 120.0 | no |

| Configured model | Tools | Outcomes | Collection seconds per behavior pass |
|---|---|---|---:|
| kimi-code-plan-global/k3 | files | 1 pending_grading | undefined |
| kimi-code-plan-global/k3 | fr | 1 pending_grading | undefined |
| zai-coding-plan/glm-5.3-flash | files | 1 failed | undefined |
| zai-coding-plan/glm-5.3-flash | fr | 1 pending_grading | undefined |

Collection time includes failed attempts; it excludes remote grading time.
Host bytes are produced results, not complete model context. See JSON for native stream confirmations,
partial usage, resource samples, tool schemas and paired outcomes. Provider billing remains unverified.
