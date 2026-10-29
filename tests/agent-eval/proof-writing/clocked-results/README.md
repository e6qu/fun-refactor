# M3: writing three fixed Lean proofs

All 36 scheduled local CLI attempts were independently checked: 25 passed and 11 failed.
Each app and model used ordinary tools, then ordinary tools plus fr, twice per task. The source, generated model,
theorem and limits were the same. Agents supplied only proof steps. Controller reference proofs were not supplied.

- Access: the model allows staff or the owner.
- Pages: invalid inputs return -1; valid inputs give a count between zero and the item count; 11 items at 5 per page give 3.
- Ranges: a range fits exactly when its offset plus length is at most the total.

Lean checks these model claims. The connection from the Rust implementation to its model remains unproved.
Separate source execution controls test finite input cases, including integer boundaries. They are not a general equivalence proof.

| Agent | Tools | Accepted / attempted | Mean successful time | Total time / success |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | files | 6 / 6 | 53.5 s | 53.5 s |
| Codex Luna Fast | fr | 5 / 6 | 50.4 s | 80.4 s |
| OpenCode2 GLM Flash | files | 4 / 6 | 60.0 s | 135.0 s |
| OpenCode2 GLM Flash | fr | 3 / 6 | 95.1 s | 245.2 s |
| OpenCode2 Kimi K3 | files | 3 / 6 | 68.2 s | 218.3 s |
| OpenCode2 Kimi K3 | fr | 4 / 6 | 86.4 s | 161.4 s |

Time includes the complete local attempt: CLI startup, model replies, tool calls, checks and patch capture.
Failed attempts stay in the totals. A valid patch delivered after the deadline still counts as a failed attempt.

| Agent | Pairs accepted in both setups | Ordinary mean | fr mean | Change with fr |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | 5 | 48.5 s | 50.4 s | 4.0% |
| OpenCode2 GLM Flash | 3 | 59.1 s | 95.1 s | 60.9% |
| OpenCode2 Kimi K3 | 3 | 68.2 s | 74.3 s | 8.8% |

Positive percentages mean fr took longer on matching successful attempts. This small comparison measures these
apps, models and requests; it does not establish general speed or money savings. Account charges are unknown.
Reported token categories retain each CLI’s meaning. Missing counters remain unknown.

| Attempt | Result | Elapsed seconds | Reported tokens | fr contribution or failure |
| --- | --- | --- | --- | --- |
| codex-luna-fast-access-rust-files-1 | passed | 26.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 55552, "input_tokens": 86132, "output_tokens": 1042, "reasoning_output_tokens": 413} | No fr contribution credited. |
| codex-luna-fast-access-rust-fr-1 | passed | 34.2 | {"cache_write_input_tokens": 0, "cached_input_tokens": 94848, "input_tokens": 109285, "output_tokens": 2041, "reasoning_output_tokens": 916} | Discovered the theorem and obtained a passing fr check for the submitted tactics. |
| codex-luna-fast-page-count-fr-1 | passed | 120.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 333568, "input_tokens": 399427, "output_tokens": 6742, "reasoning_output_tokens": 3647} | Used the proof task to inspect the fixed claim.; Used fr diagnostics to repair drafts, obtained a passing check and saved the proof with fr. |
| codex-luna-fast-page-count-files-1 | passed | 136.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 534400, "input_tokens": 604740, "output_tokens": 7464, "reasoning_output_tokens": 3277} | No fr contribution credited. |
| codex-luna-fast-fits-window-files-1 | passed | 23.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 94848, "input_tokens": 101831, "output_tokens": 1416, "reasoning_output_tokens": 667} | No fr contribution credited. |
| codex-luna-fast-fits-window-fr-1 | passed | 27.0 | {"cache_write_input_tokens": 0, "cached_input_tokens": 89088, "input_tokens": 97615, "output_tokens": 1463, "reasoning_output_tokens": 515} | Checked the proposed tactics and saved the theorem proof with fr. |
| opencode2-glm-flash-access-rust-fr-1 | passed | 101.4 | Unknown or incomplete | Discovered the theorem and checked its tactics with fr.; Verified the edited Lean package with fr. |
| opencode2-glm-flash-access-rust-files-1 | passed | 58.5 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-page-count-files-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading and searching the Lean library, before a draft check or delivered patch. |
| opencode2-glm-flash-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading and searching the Lean library, before a draft check or delivered patch. |
| opencode2-glm-flash-fits-window-fr-1 | passed | 100.0 | Unknown or incomplete | Checked the proposed tactics and saved the theorem proof with fr. |
| opencode2-glm-flash-fits-window-files-1 | passed | 55.4 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-files-1 | passed | 35.5 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-fr-1 | passed | 56.5 | Unknown or incomplete | Checked the proposed tactics and saved the theorem proof with fr. |
| opencode2-kimi-k3-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading and searching the Lean library, before a draft check or delivered patch. |
| opencode2-kimi-k3-page-count-files-1 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading and searching the Lean library, before a draft check or delivered patch. |
| opencode2-kimi-k3-fits-window-files-1 | passed | 87.4 | {"cache": {"read": 59136, "write": 0}, "input": 5244, "output": 1104, "reasoning": 1202} | No fr contribution credited. |
| opencode2-kimi-k3-fits-window-fr-1 | passed | 115.7 | Unknown or incomplete | Used fr diagnostics to repair a rejected draft, then checked and saved the proof with fr. |
| codex-luna-fast-access-rust-fr-2 | passed | 30.5 | {"cache_write_input_tokens": 0, "cached_input_tokens": 58624, "input_tokens": 78843, "output_tokens": 1570, "reasoning_output_tokens": 740} | Checked the proposed tactics and saved the theorem proof with fr. |
| codex-luna-fast-access-rust-files-2 | passed | 24.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 51584, "input_tokens": 61540, "output_tokens": 947, "reasoning_output_tokens": 303} | No fr contribution credited. |
| codex-luna-fast-page-count-files-2 | passed | 78.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 488960, "input_tokens": 525412, "output_tokens": 6562, "reasoning_output_tokens": 2899} | No fr contribution credited. |
| codex-luna-fast-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | A draft passed fr checking just before the 150-second limit; the client delivered no patch. |
| codex-luna-fast-fits-window-fr-2 | passed | 39.5 | {"cache_write_input_tokens": 0, "cached_input_tokens": 81920, "input_tokens": 97666, "output_tokens": 1877, "reasoning_output_tokens": 1068} | Checked the proposed tactics and saved the theorem proof with fr. |
| codex-luna-fast-fits-window-files-2 | passed | 31.0 | {"cache_write_input_tokens": 0, "cached_input_tokens": 107520, "input_tokens": 118172, "output_tokens": 2130, "reasoning_output_tokens": 944} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-files-2 | passed | 63.5 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-access-rust-fr-2 | passed | 83.9 | Unknown or incomplete | Checked the proposed tactics and saved the theorem proof with fr. |
| opencode2-glm-flash-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading the model and instructions, before a draft check or delivered patch. |
| opencode2-glm-flash-page-count-files-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading the model and instructions, before a draft check or delivered patch. |
| opencode2-glm-flash-fits-window-files-2 | passed | 62.5 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-fits-window-fr-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after incorrect fr calls and a direct edit removed the marked goal; the Lean package rejected the draft. No patch was delivered. |
| opencode2-kimi-k3-access-rust-fr-2 | passed | 50.6 | Unknown or incomplete | Checked the proposed tactics and saved the theorem proof with fr. |
| opencode2-kimi-k3-access-rust-files-2 | passed | 81.9 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-page-count-files-2 | failed | 150.1 | Unknown or incomplete | Reached the 150-second limit after library searches and a direct proof edit, without a completed check or delivered patch. |
| opencode2-kimi-k3-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after reading and searching the Lean library, before a draft check or delivered patch. |
| opencode2-kimi-k3-fits-window-fr-2 | passed | 122.8 | Unknown or incomplete | Obtained a passing fr check and saved the proof after correcting the command arguments. |
| opencode2-kimi-k3-fits-window-files-2 | failed | 150.0 | Unknown or incomplete | Reached the 150-second limit after Lean rejected a draft. A later edit had no completed check or delivered patch. |

The published files contain submitted patches and this summary. Raw prompts, CLI transcripts, source archives
and account data stay local. The summary records the selected model settings, tool versions, checker identity,
source hashes, collection limits and approved axioms; failures and timeouts are retained.
