# Comparison affected by system sleep

All 36 scheduled attempts closed: 18 clients completed and 18 failed. Every original outcome is retained.

The macOS timer excluded system sleep. Ten already-failed attempts show large differences between saved timestamps and recorded durations.
For example, one failed attempt recorded about 65 seconds while saved timestamps span about 1,123 seconds.
This packet cannot support a speed or cost comparison. No time limit was increased and no failed slot was retried.

These patches have not been independently graded. Client completion does not establish that a proof is correct.

| Agent | Task | Tools | Client outcome | Submitted patch |
| --- | --- | --- | --- | --- |
| codex-luna-fast | access-rust | files | completed | [Patch](patches/codex-luna-fast-access-rust-files-1.patch) |
| codex-luna-fast | access-rust | fr | completed | [Patch](patches/codex-luna-fast-access-rust-fr-1.patch) |
| codex-luna-fast | page-count | fr | completed | [Patch](patches/codex-luna-fast-page-count-fr-1.patch) |
| codex-luna-fast | page-count | files | completed | [Patch](patches/codex-luna-fast-page-count-files-1.patch) |
| codex-luna-fast | fits-window | files | completed | [Patch](patches/codex-luna-fast-fits-window-files-1.patch) |
| codex-luna-fast | fits-window | fr | completed | [Patch](patches/codex-luna-fast-fits-window-fr-1.patch) |
| opencode2-glm-flash | access-rust | fr | failed | [Patch](patches/opencode2-glm-flash-access-rust-fr-1.patch) |
| opencode2-glm-flash | access-rust | files | completed | [Patch](patches/opencode2-glm-flash-access-rust-files-1.patch) |
| opencode2-glm-flash | page-count | files | failed | None |
| opencode2-glm-flash | page-count | fr | failed | None |
| opencode2-glm-flash | fits-window | fr | failed | [Patch](patches/opencode2-glm-flash-fits-window-fr-1.patch) |
| opencode2-glm-flash | fits-window | files | failed | None |
| opencode2-kimi-k3 | access-rust | files | failed | None |
| opencode2-kimi-k3 | access-rust | fr | failed | None |
| opencode2-kimi-k3 | page-count | fr | failed | None |
| opencode2-kimi-k3 | page-count | files | failed | None |
| opencode2-kimi-k3 | fits-window | files | completed | [Patch](patches/opencode2-kimi-k3-fits-window-files-1.patch) |
| opencode2-kimi-k3 | fits-window | fr | failed | None |
| codex-luna-fast | access-rust | fr | failed | None |
| codex-luna-fast | access-rust | files | completed | [Patch](patches/codex-luna-fast-access-rust-files-2.patch) |
| codex-luna-fast | page-count | files | completed | [Patch](patches/codex-luna-fast-page-count-files-2.patch) |
| codex-luna-fast | page-count | fr | completed | [Patch](patches/codex-luna-fast-page-count-fr-2.patch) |
| codex-luna-fast | fits-window | fr | completed | [Patch](patches/codex-luna-fast-fits-window-fr-2.patch) |
| codex-luna-fast | fits-window | files | completed | [Patch](patches/codex-luna-fast-fits-window-files-2.patch) |
| opencode2-glm-flash | access-rust | files | completed | [Patch](patches/opencode2-glm-flash-access-rust-files-2.patch) |
| opencode2-glm-flash | access-rust | fr | completed | [Patch](patches/opencode2-glm-flash-access-rust-fr-2.patch) |
| opencode2-glm-flash | page-count | fr | failed | None |
| opencode2-glm-flash | page-count | files | failed | None |
| opencode2-glm-flash | fits-window | files | completed | [Patch](patches/opencode2-glm-flash-fits-window-files-2.patch) |
| opencode2-glm-flash | fits-window | fr | failed | None |
| opencode2-kimi-k3 | access-rust | fr | failed | [Patch](patches/opencode2-kimi-k3-access-rust-fr-2.patch) |
| opencode2-kimi-k3 | access-rust | files | completed | [Patch](patches/opencode2-kimi-k3-access-rust-files-2.patch) |
| opencode2-kimi-k3 | page-count | files | failed | None |
| opencode2-kimi-k3 | page-count | fr | failed | None |
| opencode2-kimi-k3 | fits-window | fr | failed | None |
| opencode2-kimi-k3 | fits-window | files | completed | [Patch](patches/opencode2-kimi-k3-fits-window-files-2.patch) |

[Summary and exact source identities](summary.json). Raw records remain local.
