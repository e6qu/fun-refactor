# Interrupted first M3 run

Seven of the planned 36 attempts ran before the resource monitor failed. The other 29 were never started.
Five clients finished and delivered patches; two attempts failed. None of these patches has an independent acceptance result.

The monitor launched `ps` with a two-second deadline. The workstation CPU guard paused the collector,
consuming that deadline before OpenCode produced output. The corrected monitor reads the macOS process group directly.
Fr also rejected valid simplifier locations such as `simp [rule] at h` and lacked equality transport syntax.
The reader fixes have separate Lean-checked regression examples. The tasks, models and resource limits remain unchanged.

The complete comparison starts from a newly frozen plan after those fixes pass their controls.
This interrupted run remains separate, including its failures and all five original patches.

| Attempt | Native result | Seconds | What happened |
| --- | --- | ---: | --- |
| codex-luna-fast-access-rust-files-1 | failed | 17.39 | A broad standard-library search reached the unchanged 1 MiB transcript cap; no patch was delivered. |
| codex-luna-fast-access-rust-fr-1 | completed | 35.07 | Disclosed the fixed goal, checked the tactics and saved the proof through fr. |
| codex-luna-fast-page-count-fr-1 | completed | 80.65 | Used the proof task, Lean diagnostics and checked write to replace the unfinished proof. |
| codex-luna-fast-page-count-files-1 | completed | 66.97 | Delivered a proof patch using ordinary tools. |
| codex-luna-fast-fits-window-files-1 | completed | 27.41 | Delivered a proof patch using ordinary tools. |
| codex-luna-fast-fits-window-fr-1 | completed | 97.37 | Used proof diagnostics and checked write; fr rejected several tactic fragments before another proof was saved. |
| opencode2-glm-flash-access-rust-fr-1 | failed | 3.19 | The resource monitor hit its two-second ps deadline while CPU throttling paused the collector; no output or patch was delivered. |

These attempts took 328.06 seconds in total, including failures.
Reported token counts are retained in [the summary](summary.json). Missing usage and account charges remain unknown.
Raw records stay local. A model proof still leaves the connection to the Rust implementation unproved.
