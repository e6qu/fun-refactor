# Interrupted comparison: valid proof introductions

The comparison stopped after 18 of 36 scheduled attempts. The other 18 never started.
12 clients finished within the deadline; 6 attempts failed. A finished client is not an accepted proof.

Kimi wrote a draft using `rintro ⟨ha, -, -, -⟩`. This introduces several hypotheses and discards
the unused ones. Fr rejected that syntax before Lean could check the draft. A separate small
example using the same form passed pinned Lean 4.28.0. The full agent draft has no accepted proof result.

The coordinator stopped after its current attempt. Every completed or failed record stays unchanged.
The parser repair requires a newly checked executable and frozen comparison. No failed slot was retried,
no unfinished slot was counted as a failure, and no time or resource limit was increased.

Five attempts reached the 150-second limit. One GLM attempt lost its provider connection and exited
with an error after writing a patch; it remains failed even if that patch later passes checking.
The Luna page-count attempt with fr delivered an edited patch after Lean rejected its last checked draft.

These are proof-writing attempts, compared with the same app and model using ordinary tools.
They do not establish execution-speed, token or billing savings. Actual charges remain unknown.

| Agent | Task | Tools | Client outcome | Seconds | Submitted patch |
| --- | --- | --- | --- | ---: | --- |
| codex-luna-fast | access-rust | files | completed | 26.92 | [Patch](patches/codex-luna-fast-access-rust-files-1.patch) |
| codex-luna-fast | access-rust | fr | completed | 24.09 | [Patch](patches/codex-luna-fast-access-rust-fr-1.patch) |
| codex-luna-fast | page-count | fr | completed | 145.82 | [Patch](patches/codex-luna-fast-page-count-fr-1.patch) |
| codex-luna-fast | page-count | files | completed | 86.93 | [Patch](patches/codex-luna-fast-page-count-files-1.patch) |
| codex-luna-fast | fits-window | files | completed | 47.77 | [Patch](patches/codex-luna-fast-fits-window-files-1.patch) |
| codex-luna-fast | fits-window | fr | completed | 30.09 | [Patch](patches/codex-luna-fast-fits-window-fr-1.patch) |
| opencode2-glm-flash | access-rust | fr | completed | 117.10 | [Patch](patches/opencode2-glm-flash-access-rust-fr-1.patch) |
| opencode2-glm-flash | access-rust | files | failed | 134.74 | [Patch](patches/opencode2-glm-flash-access-rust-files-1.patch) |
| opencode2-glm-flash | page-count | files | failed | 150.06 | None |
| opencode2-glm-flash | page-count | fr | failed | 150.04 | None |
| opencode2-glm-flash | fits-window | fr | completed | 131.81 | [Patch](patches/opencode2-glm-flash-fits-window-fr-1.patch) |
| opencode2-glm-flash | fits-window | files | failed | 150.03 | None |
| opencode2-kimi-k3 | access-rust | files | completed | 33.77 | [Patch](patches/opencode2-kimi-k3-access-rust-files-1.patch) |
| opencode2-kimi-k3 | access-rust | fr | completed | 52.95 | [Patch](patches/opencode2-kimi-k3-access-rust-fr-1.patch) |
| opencode2-kimi-k3 | page-count | fr | failed | 150.04 | None |
| opencode2-kimi-k3 | page-count | files | failed | 150.03 | None |
| opencode2-kimi-k3 | fits-window | files | completed | 64.52 | [Patch](patches/opencode2-kimi-k3-fits-window-files-1.patch) |
| opencode2-kimi-k3 | fits-window | fr | completed | 103.11 | [Patch](patches/opencode2-kimi-k3-fits-window-fr-1.patch) |

[Summary and original source identities](summary.json). Only patches and summaries are public; raw records stay local.
