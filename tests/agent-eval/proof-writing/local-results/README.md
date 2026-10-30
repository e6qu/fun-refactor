# M3: writing three fixed Lean proofs

All 36 scheduled local CLI attempts were independently checked: 26 passed and 10 failed.

Still missing accepted proofs using fr:

- OpenCode2 GLM Flash: Safe page counts.
- OpenCode2 Kimi K3: Safe page counts.

Each app and model used ordinary tools, then ordinary tools plus fr, twice per task. The source, generated model,
theorem and limits were the same. Agents supplied only proof steps. Controller reference proofs were not supplied.

- Access: the model allows staff or the owner.
- Pages: invalid inputs return -1; valid inputs give a count between zero and the item count; 11 items at 5 per page give 3.
- Ranges: a range fits exactly when its offset plus length is at most the total.

Lean checks these model claims. The connection from the Rust implementation to its model remains unproved.
Separate source execution controls test finite input cases, including integer boundaries. They are not a general equivalence proof.

| Agent | Tools | Accepted / attempted | Mean successful time | Total time / success |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | files | 6 / 6 | 43.7 s | 43.7 s |
| Codex Luna Fast | fr | 5 / 6 | 36.5 s | 65.0 s |
| OpenCode2 GLM Flash | files | 4 / 6 | 53.5 s | 128.5 s |
| OpenCode2 GLM Flash | fr | 3 / 6 | 49.6 s | 168.0 s |
| OpenCode2 Kimi K3 | files | 4 / 6 | 53.4 s | 128.4 s |
| OpenCode2 Kimi K3 | fr | 4 / 6 | 56.2 s | 131.2 s |

Time includes the complete local attempt: CLI startup, model replies, tool calls, checks and patch capture.
Failed attempts stay in the totals. A valid patch delivered after the deadline still counts as a failed attempt.

`codex-luna-fast-page-count-fr-2` includes 42.9 seconds of detected system sleep in its 142.9-second total.

| Agent | Pairs accepted in both setups | Ordinary mean | fr mean | Change with fr |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | 5 | 34.7 s | 36.5 s | 5.1% |
| OpenCode2 GLM Flash | 3 | 54.3 s | 49.6 s | -8.6% |
| OpenCode2 Kimi K3 | 4 | 53.4 s | 56.2 s | 5.2% |

Positive percentages mean fr took longer on matching successful attempts. This small comparison measures these
apps, models and requests; it does not establish general speed or money savings. Account charges are unknown.
Reported token categories retain each CLI’s meaning. Missing counters remain unknown.

| Attempt | Result | Elapsed seconds | Reported tokens | fr contribution or failure |
| --- | --- | --- | --- | --- |
| codex-luna-fast-access-rust-files-1 | passed | 27.6 | {"cache_write_input_tokens": 0, "cached_input_tokens": 59520, "input_tokens": 87454, "output_tokens": 1453, "reasoning_output_tokens": 690} | No fr contribution credited. |
| codex-luna-fast-access-rust-fr-1 | passed | 26.5 | {"cache_write_input_tokens": 0, "cached_input_tokens": 79360, "input_tokens": 112108, "output_tokens": 1524, "reasoning_output_tokens": 656} | Found the fixed theorem, checked and saved the tactics with fr, then built the Lean package and delivered the patch |
| codex-luna-fast-page-count-fr-1 | passed | 69.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 369280, "input_tokens": 440311, "output_tokens": 5827, "reasoning_output_tokens": 3347} | Found the fixed goal and proof task with fr, used Lean diagnostics to repair rejected drafts, then obtained a passing fr check and built the Lean package |
| codex-luna-fast-page-count-files-1 | passed | 75.3 | {"cache_write_input_tokens": 0, "cached_input_tokens": 290048, "input_tokens": 328838, "output_tokens": 5917, "reasoning_output_tokens": 3071} | No fr contribution credited. |
| codex-luna-fast-fits-window-files-1 | passed | 27.0 | {"cache_write_input_tokens": 0, "cached_input_tokens": 77056, "input_tokens": 92786, "output_tokens": 1421, "reasoning_output_tokens": 656} | No fr contribution credited. |
| codex-luna-fast-fits-window-fr-1 | passed | 30.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 97920, "input_tokens": 116454, "output_tokens": 2307, "reasoning_output_tokens": 1108} | Found the fixed range-check theorem, checked and saved its tactics with fr, then built the Lean package and delivered the patch |
| opencode2-glm-flash-access-rust-fr-1 | passed | 49.1 | Unknown or incomplete | Found the fixed access goal, obtained a passing fr check, saved the tactics with fr and built the Lean package |
| opencode2-glm-flash-access-rust-files-1 | passed | 42.2 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-page-count-files-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after three Lean checks rejected arithmetic drafts. No submitted patch was delivered |
| opencode2-glm-flash-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit while searching division-library declarations. No fr command, draft check or submitted patch was observed |
| opencode2-glm-flash-fits-window-fr-1 | failed | 55.1 | Unknown or incomplete | Obtained a passing fr check, saved the range-check proof with fr and built the Lean package; Deleted the supplied .fr/checks.json during cleanup. The controller retained the submitted patch but correctly marked the entire attempt failed |
| opencode2-glm-flash-fits-window-files-1 | passed | 51.3 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-files-1 | passed | 43.1 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-fr-1 | passed | 32.6 | Unknown or incomplete | Found the fixed access theorem, obtained a passing fr check, saved its tactics with fr and built the Lean package |
| opencode2-kimi-k3-page-count-fr-1 | failed | 150.1 | Unknown or incomplete | Reached the 150-second limit while reading the model and division-library declarations. No fr command, draft check or submitted patch was observed |
| opencode2-kimi-k3-page-count-files-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after library searches and a Lean check rejected an arithmetic draft. No submitted patch was delivered |
| opencode2-kimi-k3-fits-window-files-1 | passed | 40.3 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-fits-window-fr-1 | passed | 51.3 | Unknown or incomplete | Found the fixed range-check goal, obtained a passing fr check, saved the tactics with fr and built the Lean package |
| codex-luna-fast-access-rust-fr-2 | passed | 25.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 105088, "input_tokens": 135268, "output_tokens": 1779, "reasoning_output_tokens": 604} | Found the exact theorem and used fr diagnostics to remove an extra by keyword, then obtained a passing check and built the Lean package |
| codex-luna-fast-access-rust-files-2 | passed | 15.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 55040, "input_tokens": 60551, "output_tokens": 982, "reasoning_output_tokens": 358} | No fr contribution credited. |
| codex-luna-fast-page-count-files-2 | passed | 88.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 314368, "input_tokens": 338324, "output_tokens": 5331, "reasoning_output_tokens": 2504} | No fr contribution credited. |
| codex-luna-fast-page-count-fr-2 | failed | 142.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 386944, "input_tokens": 422660, "output_tokens": 8260, "reasoning_output_tokens": 5027} | Used fr diagnostics to revise rejected arithmetic drafts; the last fr check still rejected the submitted proof; Delivered a proof rejected by independent checks after its last fr check failed. Its 142.9-second total includes 42.9 seconds of detected system sleep. |
| codex-luna-fast-fits-window-fr-2 | passed | 30.2 | {"cache_write_input_tokens": 0, "cached_input_tokens": 101632, "input_tokens": 136889, "output_tokens": 2022, "reasoning_output_tokens": 938} | Found the fixed range-check goal, obtained a passing fr check, saved the tactics with fr and built the Lean package |
| codex-luna-fast-fits-window-files-2 | passed | 28.3 | {"cache_write_input_tokens": 0, "cached_input_tokens": 82944, "input_tokens": 90016, "output_tokens": 1471, "reasoning_output_tokens": 679} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-files-2 | passed | 56.3 | {"cache": {"read": 33408, "write": 0}, "input": 7429, "output": 378, "reasoning": 0} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-fr-2 | passed | 45.9 | {"cache": {"read": 52096, "write": 0}, "input": 9176, "output": 421, "reasoning": 0} | Found the exact access theorem, obtained a passing fr check, saved its tactics with fr and built the Lean package |
| opencode2-glm-flash-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after library searches and a late fr goal lookup. No draft check or submitted patch was observed |
| opencode2-glm-flash-page-count-files-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit while reading division-library declarations. No checked draft or submitted patch was observed |
| opencode2-glm-flash-fits-window-files-2 | passed | 64.3 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-fits-window-fr-2 | passed | 53.8 | Unknown or incomplete | Found the exact range-check goal, obtained a passing fr check and saved the proof with fr before the Lean build and patch delivery |
| opencode2-kimi-k3-access-rust-fr-2 | passed | 82.7 | Unknown or incomplete | Obtained a passing fr check after correcting two guessed theorem targets, saved the tactics with fr and built the Lean package |
| opencode2-kimi-k3-access-rust-files-2 | passed | 31.4 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-page-count-files-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after library searches and three rejected Lean builds. No submitted patch was delivered |
| opencode2-kimi-k3-page-count-fr-2 | failed | 150.1 | Unknown or incomplete | Reached the 150-second limit while reading the model and division declarations. No fr check or submitted patch was observed |
| opencode2-kimi-k3-fits-window-fr-2 | passed | 58.0 | Unknown or incomplete | Found the fixed range-check goal, obtained a passing fr check, saved its tactics with fr and built the Lean package |
| opencode2-kimi-k3-fits-window-files-2 | passed | 98.7 | Unknown or incomplete | No fr contribution credited. |

The published files contain submitted patches and this summary. Raw prompts, CLI transcripts, source archives
and account data stay local. The summary records the selected model settings, tool versions, checker identity,
source hashes, collection limits and approved axioms; failures and timeouts are retained.

The [behavior review](../behavior-review.md) examines GLM and Kimi commands and timing.
