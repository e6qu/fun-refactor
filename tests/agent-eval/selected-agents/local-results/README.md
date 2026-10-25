# M1: the same agents with and without fr

The three requests are a Boltons rename, a Unicode text-comparison repair, and a Lean proof of the fixed access rule.
Each app/model gets two attempts per request with ordinary tools and two with ordinary tools plus fr: 36 scheduled attempts.
A completed CLI is not an independently accepted change. GitHub checks each submitted patch against the original source.
Patches and this summary are public. The full local records remain local, as requested.

All timeouts and failures stay in the table. The 150-second task limit and the outer 180-second guard are unchanged.
The first GLM/fr rename reached its task limit after 12 short tool steps; model generation took most of its time. It submitted no patch.
Its interrupted-session recorder assumed a final outcome existed. That recording error is retained and fixed; model-facing settings and limits are unchanged.

Token categories retain each CLI's own meaning. Compare matching profiles; unavailable account charges are unknown.
Fresh task setup, instructions, indexing, tools, checks and export are inside local-attempt time. Shared setup is listed separately in summary.json.
Operating-system and provider caches are uncontrolled. A tool being available, or a command being called, does not establish a benefit.

Each proof attempt starts with the same generated model and unfinished claim. Model generation is shared preparation.

| Agent | Task | Tools | Repeat | Check result | Local seconds | Patch |
| --- | --- | --- | ---: | --- | ---: | --- |
| Codex Luna Fast | Python rename | ordinary | 1 | passed | 32.814 | [patch](patches/codex-luna-fast-boltons-rename-files-1.patch) |
| Codex Luna Fast | Python rename | ordinary + fr | 1 | passed | 46.427 | [patch](patches/codex-luna-fast-boltons-rename-fr-1.patch) |
| Codex Luna Fast | Unicode repair | ordinary + fr | 1 | passed | 30.634 | [patch](patches/codex-luna-fast-unicode-dice-fr-1.patch) |
| Codex Luna Fast | Unicode repair | ordinary | 1 | passed | 32.464 | [patch](patches/codex-luna-fast-unicode-dice-files-1.patch) |
| Codex Luna Fast | Access-rule proof | ordinary | 1 | passed | 22.043 | [patch](patches/codex-luna-fast-access-proof-files-1.patch) |
| Codex Luna Fast | Access-rule proof | ordinary + fr | 1 | passed | 38.488 | [patch](patches/codex-luna-fast-access-proof-fr-1.patch) |
| OpenCode2 GLM Flash | Python rename | ordinary + fr | 1 | failed | 151.019 | none |
| OpenCode2 GLM Flash | Python rename | ordinary | 1 | time limit | 150.957 | none |
| OpenCode2 GLM Flash | Unicode repair | ordinary | 1 | time limit | 150.962 | none |
| OpenCode2 GLM Flash | Unicode repair | ordinary + fr | 1 | time limit | 150.911 | none |
| OpenCode2 GLM Flash | Access-rule proof | ordinary + fr | 1 | awaiting check | 113.755 | [patch](patches/opencode2-glm-flash-access-proof-fr-1.patch) |
| OpenCode2 GLM Flash | Access-rule proof | ordinary | 1 | awaiting check | 111.324 | [patch](patches/opencode2-glm-flash-access-proof-files-1.patch) |
| OpenCode2 Kimi K3 | Python rename | ordinary | 1 | awaiting check | 93.213 | [patch](patches/opencode2-kimi-k3-boltons-rename-files-1.patch) |
| OpenCode2 Kimi K3 | Python rename | ordinary + fr | 1 | awaiting check | 135.421 | [patch](patches/opencode2-kimi-k3-boltons-rename-fr-1.patch) |
| OpenCode2 Kimi K3 | Unicode repair | ordinary + fr | 1 | time limit | 150.81 | none |
| OpenCode2 Kimi K3 | Unicode repair | ordinary | 1 | awaiting check | 136.807 | [patch](patches/opencode2-kimi-k3-unicode-dice-files-1.patch) |
| OpenCode2 Kimi K3 | Access-rule proof | ordinary | 1 | not started | — | none |
| OpenCode2 Kimi K3 | Access-rule proof | ordinary + fr | 1 | not started | — | none |
| Codex Luna Fast | Python rename | ordinary + fr | 2 | not started | — | none |
| Codex Luna Fast | Python rename | ordinary | 2 | not started | — | none |
| Codex Luna Fast | Unicode repair | ordinary | 2 | not started | — | none |
| Codex Luna Fast | Unicode repair | ordinary + fr | 2 | not started | — | none |
| Codex Luna Fast | Access-rule proof | ordinary + fr | 2 | not started | — | none |
| Codex Luna Fast | Access-rule proof | ordinary | 2 | not started | — | none |
| OpenCode2 GLM Flash | Python rename | ordinary | 2 | not started | — | none |
| OpenCode2 GLM Flash | Python rename | ordinary + fr | 2 | not started | — | none |
| OpenCode2 GLM Flash | Unicode repair | ordinary + fr | 2 | not started | — | none |
| OpenCode2 GLM Flash | Unicode repair | ordinary | 2 | not started | — | none |
| OpenCode2 GLM Flash | Access-rule proof | ordinary | 2 | not started | — | none |
| OpenCode2 GLM Flash | Access-rule proof | ordinary + fr | 2 | not started | — | none |
| OpenCode2 Kimi K3 | Python rename | ordinary + fr | 2 | not started | — | none |
| OpenCode2 Kimi K3 | Python rename | ordinary | 2 | not started | — | none |
| OpenCode2 Kimi K3 | Unicode repair | ordinary | 2 | not started | — | none |
| OpenCode2 Kimi K3 | Unicode repair | ordinary + fr | 2 | not started | — | none |
| OpenCode2 Kimi K3 | Access-rule proof | ordinary + fr | 2 | not started | — | none |
| OpenCode2 Kimi K3 | Access-rule proof | ordinary | 2 | not started | — | none |

Independent outcomes will be added from the GitHub checker. A model proof does not establish the original source/model connection.
See [PLAN.md](../../../../PLAN.md#m1-run-the-selected-agents-on-three-complete-tasks) for completion conditions.
