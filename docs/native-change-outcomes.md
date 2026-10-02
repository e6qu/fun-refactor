# Native code-change outcomes and observed costs

These reviewed integration tasks establish no general efficiency advantage.
Failed attempts remain in cost totals. Missing totals are unknown, not zero.

## 2026-10-02-code-changes

Plan: `76243d6223c4b764fb85a16d2437715b2b611a45e211f350af5562f9046c6217`.

| Configured model | Tools available | Outcome | Host calls | fr calls | Result bytes | Collection seconds | CLI steps complete |
|---|---|---|---:|---:|---:|---:|---|
| zai-coding-plan/glm-5.3-flash | ordinary files + fr | collection failed | 8 | 0 | 19095 | 120.1 | no |
| zai-coding-plan/glm-5.3-flash | ordinary files | behavior pass | 11 | 0 | 19536 | 91.5 | yes |
| kimi-code-plan-global/k3 | ordinary files | behavior pass | 4 | 0 | 10509 | 64.4 | yes |
| kimi-code-plan-global/k3 | ordinary files + fr | behavior pass | 5 | 0 | 10741 | 53.0 | yes |
| kimi-code-plan-global/k3 | ordinary files | behavior pass | 6 | 0 | 11912 | 34.5 | yes |
| kimi-code-plan-global/k3 | ordinary files + fr | behavior pass | 10 | 0 | 26438 | 91.8 | yes |
| zai-coding-plan/glm-5.3-flash | ordinary files | behavior pass | 11 | 0 | 10738 | 105.5 | yes |
| zai-coding-plan/glm-5.3-flash | ordinary files + fr | behavior pass | 13 | 0 | 18733 | 107.3 | yes |

| Configured model | Tools available | Outcomes | Collection seconds per behavior pass |
|---|---|---|---:|
| kimi-code-plan-global/k3 | ordinary files | 2 behavior pass | 49.5 |
| kimi-code-plan-global/k3 | ordinary files + fr | 2 behavior pass | 72.4 |
| zai-coding-plan/glm-5.3-flash | ordinary files | 2 behavior pass | 98.5 |
| zai-coding-plan/glm-5.3-flash | ordinary files + fr | 1 collection failed, 1 behavior pass | 227.4 |

Collection time includes failed attempts; it excludes remote grading time.
Host bytes are produced results, not complete model context. See JSON for native stream confirmations,
partial usage, resource samples, tool schemas and paired outcomes. Provider billing remains unverified.

## 2026-10-02-public-edit-code-changes

Plan: `5217e3521e71dc2b78de5bd787b6631db8c321f83cca13cce05658a3a4f699dd`.

| Configured model | Tools available | Outcome | Host calls | fr calls | Result bytes | Collection seconds | CLI steps complete |
|---|---|---|---:|---:|---:|---:|---|
| kimi-code-plan-global/k3 | ordinary files + fr | behavior pass | 6 | 0 | 3281 | 65.7 | yes |
| kimi-code-plan-global/k3 | ordinary files | behavior pass | 7 | 0 | 16052 | 59.2 | yes |
| zai-coding-plan/glm-5.3-flash | ordinary files + fr | behavior pass | 10 | 0 | 5626 | 100.6 | yes |
| zai-coding-plan/glm-5.3-flash | ordinary files | collection failed | 12 | 0 | 33864 | 120.0 | no |

| Configured model | Tools available | Outcomes | Collection seconds per behavior pass |
|---|---|---|---:|
| kimi-code-plan-global/k3 | ordinary files | 1 behavior pass | 59.2 |
| kimi-code-plan-global/k3 | ordinary files + fr | 1 behavior pass | 65.7 |
| zai-coding-plan/glm-5.3-flash | ordinary files | 1 collection failed | undefined |
| zai-coding-plan/glm-5.3-flash | ordinary files + fr | 1 behavior pass | 100.6 |

Collection time includes failed attempts; it excludes remote grading time.
Host bytes are produced results, not complete model context. See JSON for native stream confirmations,
partial usage, resource samples, tool schemas and paired outcomes. Provider billing remains unverified.
