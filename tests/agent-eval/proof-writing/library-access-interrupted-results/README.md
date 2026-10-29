# Interrupted comparison: Lean library access

This packet stopped after 15 of 36 scheduled attempts. The other 21 never started.

The instructions allowed reading the installed Lean standard library. OpenCode2 denied a command
that entered that directory because the setup had not explicitly allowed external-directory access.
An absolute-path search worked later; the observed denial does not establish that all library reads failed.
The denied command and later timeout do not establish how much time this setup error cost.

The coordinator was paused while its current bounded attempt finished, then stopped.
Every native result and submitted patch stays unchanged. Changing the permissions requires a new frozen comparison.
All time, memory and output limits remain unchanged.

These patches have not been independently graded. A completed client response is not an accepted proof.

| Agent | Task | Tools | Client outcome | Seconds | Submitted patch |
| --- | --- | --- | --- | ---: | --- |
| codex-luna-fast | access-rust | files | completed | 25.04 | [Patch](patches/codex-luna-fast-access-rust-files-1.patch) |
| codex-luna-fast | access-rust | fr | completed | 26.40 | [Patch](patches/codex-luna-fast-access-rust-fr-1.patch) |
| codex-luna-fast | page-count | fr | completed | 109.52 | [Patch](patches/codex-luna-fast-page-count-fr-1.patch) |
| codex-luna-fast | page-count | files | completed | 89.52 | [Patch](patches/codex-luna-fast-page-count-files-1.patch) |
| codex-luna-fast | fits-window | files | completed | 38.06 | [Patch](patches/codex-luna-fast-fits-window-files-1.patch) |
| codex-luna-fast | fits-window | fr | completed | 24.77 | [Patch](patches/codex-luna-fast-fits-window-fr-1.patch) |
| opencode2-glm-flash | access-rust | fr | completed | 89.92 | [Patch](patches/opencode2-glm-flash-access-rust-fr-1.patch) |
| opencode2-glm-flash | access-rust | files | completed | 52.11 | [Patch](patches/opencode2-glm-flash-access-rust-files-1.patch) |
| opencode2-glm-flash | page-count | files | failed | 150.04 | None |
| opencode2-glm-flash | page-count | fr | failed | 150.04 | None |
| opencode2-glm-flash | fits-window | fr | completed | 77.32 | [Patch](patches/opencode2-glm-flash-fits-window-fr-1.patch) |
| opencode2-glm-flash | fits-window | files | completed | 62.02 | [Patch](patches/opencode2-glm-flash-fits-window-files-1.patch) |
| opencode2-kimi-k3 | access-rust | files | completed | 30.25 | [Patch](patches/opencode2-kimi-k3-access-rust-files-1.patch) |
| opencode2-kimi-k3 | access-rust | fr | completed | 33.73 | [Patch](patches/opencode2-kimi-k3-access-rust-fr-1.patch) |
| opencode2-kimi-k3 | page-count | fr | failed | 150.02 | None |

[Summary and exact source identities](summary.json). Only patches and summaries are public; raw records stay local.
