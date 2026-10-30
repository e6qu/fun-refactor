# M3: writing three fixed Lean proofs

All 36 scheduled local CLI attempts were independently checked: 26 passed and 10 failed.
Each app and model used ordinary tools, then ordinary tools plus fr, twice per task. The source, generated model,
theorem and limits were the same. Agents supplied only proof steps. Controller reference proofs were not supplied.

- Access: the model allows staff or the owner.
- Pages: invalid inputs return -1; valid inputs give a count between zero and the item count; 11 items at 5 per page give 3.
- Ranges: a range fits exactly when its offset plus length is at most the total.

Lean checks these model claims. The connection from the Rust implementation to its model remains unproved.
Separate source execution controls test finite input cases, including integer boundaries. They are not a general equivalence proof.

| Agent | Tools | Accepted / attempted | Mean successful time | Total time / success |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | files | 6 / 6 | 54.8 s | 54.8 s |
| Codex Luna Fast | fr | 5 / 6 | 51.2 s | 80.0 s |
| OpenCode2 GLM Flash | files | 3 / 6 | 63.1 s | 213.1 s |
| OpenCode2 GLM Flash | fr | 4 / 6 | 71.9 s | 147.0 s |
| OpenCode2 Kimi K3 | files | 4 / 6 | 64.4 s | 139.4 s |
| OpenCode2 Kimi K3 | fr | 4 / 6 | 64.3 s | 139.4 s |

Time includes the complete local attempt: CLI startup, model replies, tool calls, checks and patch capture.
Failed attempts stay in the totals. A valid patch delivered after the deadline still counts as a failed attempt.

| Agent | Pairs accepted in both setups | Ordinary mean | fr mean | Change with fr |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | 5 | 41.7 s | 51.2 s | 22.6% |
| OpenCode2 GLM Flash | 3 | 63.1 s | 69.0 s | 9.4% |
| OpenCode2 Kimi K3 | 4 | 64.4 s | 64.3 s | -0.1% |

Positive percentages mean fr took longer on matching successful attempts. This small comparison measures these
apps, models and requests; it does not establish general speed or money savings. Account charges are unknown.
Reported token categories retain each CLI’s meaning. Missing counters remain unknown.

| Attempt | Result | Elapsed seconds | Reported tokens | fr contribution or failure |
| --- | --- | --- | --- | --- |
| codex-luna-fast-access-rust-files-1 | passed | 27.3 | {"cache_write_input_tokens": 0, "cached_input_tokens": 47104, "input_tokens": 85997, "output_tokens": 1309, "reasoning_output_tokens": 557} | No fr contribution credited. |
| codex-luna-fast-access-rust-fr-1 | passed | 32.2 | {"cache_write_input_tokens": 0, "cached_input_tokens": 81408, "input_tokens": 99149, "output_tokens": 1692, "reasoning_output_tokens": 858} | Found the fixed theorem and saved checked tactics with fr, then built the Lean package and obtained a passing fr recheck |
| codex-luna-fast-page-count-fr-1 | passed | 103.5 | {"cache_write_input_tokens": 0, "cached_input_tokens": 534144, "input_tokens": 567776, "output_tokens": 8740, "reasoning_output_tokens": 4260} | Discovered the fixed goal with fr and used its Lean diagnostics while repairing rejected arithmetic drafts; Obtained a passing fr check after correcting the proof and successfully building the Lean package |
| codex-luna-fast-page-count-files-1 | passed | 100.6 | {"cache_write_input_tokens": 0, "cached_input_tokens": 342784, "input_tokens": 392665, "output_tokens": 5762, "reasoning_output_tokens": 2586} | No fr contribution credited. |
| codex-luna-fast-fits-window-files-1 | passed | 25.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 53504, "input_tokens": 86365, "output_tokens": 1353, "reasoning_output_tokens": 551} | No fr contribution credited. |
| codex-luna-fast-fits-window-fr-1 | passed | 24.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 74112, "input_tokens": 94396, "output_tokens": 1537, "reasoning_output_tokens": 566} | Found the fixed goal, checked the arithmetic tactics and saved the passing proof with fr before building the Lean package |
| opencode2-glm-flash-access-rust-fr-1 | passed | 65.0 | Unknown or incomplete | Corrected the theorem target using fr's goal listing, obtained a passing check and saved the tactics with fr |
| opencode2-glm-flash-access-rust-files-1 | passed | 52.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-page-count-files-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after library searches and a Lean build rejected its arithmetic draft. No submitted patch was delivered. |
| opencode2-glm-flash-page-count-fr-1 | failed | 150.1 | Unknown or incomplete | Reached the 150-second limit after goal discovery and a division-library lookup with fr. No checked draft or submitted patch was observed. |
| opencode2-glm-flash-fits-window-fr-1 | passed | 80.8 | Unknown or incomplete | Found the fixed goal, obtained a passing fr check and saved the proof with fr before building the Lean package |
| opencode2-glm-flash-fits-window-files-1 | failed | 150.1 | Unknown or incomplete | Reached the 150-second limit after Lean rejected successive range-check drafts. No submitted patch was delivered. |
| opencode2-kimi-k3-access-rust-files-1 | passed | 47.3 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-fr-1 | passed | 59.8 | Unknown or incomplete | Found the fixed theorem, checked and saved the tactics with fr, then built the Lean package and delivered the patch |
| opencode2-kimi-k3-page-count-fr-1 | failed | 150.1 | Unknown or incomplete | Reached the 150-second limit after finding the fixed goal with fr. No draft check or submitted patch was observed. |
| opencode2-kimi-k3-page-count-files-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading the task and searching integer-division library declarations. No submitted patch was delivered. |
| opencode2-kimi-k3-fits-window-files-1 | passed | 56.5 | {"cache": {"read": 40704, "write": 0}, "input": 4100, "output": 1030, "reasoning": 625} | No fr contribution credited. |
| opencode2-kimi-k3-fits-window-fr-1 | passed | 76.3 | Unknown or incomplete | Found the fixed range-check goal, checked and saved the tactics with fr, then built the Lean package and delivered the patch |
| codex-luna-fast-access-rust-fr-2 | passed | 44.4 | {"cache_write_input_tokens": 0, "cached_input_tokens": 109056, "input_tokens": 123125, "output_tokens": 2633, "reasoning_output_tokens": 1287} | Found the fixed theorem with fr and obtained a passing check after removing an extra by keyword; saved the proof and built the Lean package |
| codex-luna-fast-access-rust-files-2 | passed | 26.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 77568, "input_tokens": 89559, "output_tokens": 1322, "reasoning_output_tokens": 536} | No fr contribution credited. |
| codex-luna-fast-page-count-files-2 | passed | 119.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 496256, "input_tokens": 559650, "output_tokens": 8514, "reasoning_output_tokens": 5393} | No fr contribution credited. |
| codex-luna-fast-page-count-fr-2 | failed | 144.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 586496, "input_tokens": 651151, "output_tokens": 11317, "reasoning_output_tokens": 7727} | Used fr's Lean diagnostics to revise arithmetic drafts; the final fr check and Lean build still rejected the proof |
| codex-luna-fast-fits-window-fr-2 | passed | 51.0 | {"cache_write_input_tokens": 0, "cached_input_tokens": 194688, "input_tokens": 217375, "output_tokens": 3486, "reasoning_output_tokens": 1945} | Used fr diagnostics to correct a rejected range-check draft, obtained a passing check, saved the tactics with fr and built the Lean package |
| codex-luna-fast-fits-window-files-2 | passed | 28.2 | {"cache_write_input_tokens": 0, "cached_input_tokens": 70912, "input_tokens": 98544, "output_tokens": 1633, "reasoning_output_tokens": 735} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-files-2 | passed | 41.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-access-rust-fr-2 | passed | 70.1 | Unknown or incomplete | Obtained a passing fr check after correcting two guessed theorem targets, saved the tactics with fr and built the Lean package |
| opencode2-glm-flash-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after inspecting the task and searching integer-division declarations. No fr draft check or submitted patch was observed. |
| opencode2-glm-flash-page-count-files-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading the task and searching division-library declarations. No submitted patch was delivered. |
| opencode2-glm-flash-fits-window-files-2 | passed | 94.7 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-fits-window-fr-2 | passed | 71.9 | Unknown or incomplete | Found the fixed range-check goal, obtained a passing fr check, saved the proof with fr and built the Lean package |
| opencode2-kimi-k3-access-rust-fr-2 | passed | 45.7 | Unknown or incomplete | Found the fixed access theorem, obtained a passing fr check, saved the proof with fr and built the Lean package |
| opencode2-kimi-k3-access-rust-files-2 | passed | 36.0 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-page-count-files-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading integer-division declarations. No checked draft or submitted patch was observed. |
| opencode2-kimi-k3-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading the task, searching division declarations and finding the goal with fr. No draft check or submitted patch was observed. |
| opencode2-kimi-k3-fits-window-fr-2 | passed | 75.6 | Unknown or incomplete | Found the range-check goal, obtained a passing fr check, saved the tactics with fr after removing an unsupported flag, and built the Lean package |
| opencode2-kimi-k3-fits-window-files-2 | passed | 117.9 | Unknown or incomplete | No fr contribution credited. |

The published files contain submitted patches and this summary. Raw prompts, CLI transcripts, source archives
and account data stay local. The summary records the selected model settings, tool versions, checker identity,
source hashes, collection limits and approved axioms; failures and timeouts are retained.
