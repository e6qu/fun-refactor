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
| Codex Luna Fast | files | 6 / 6 | 57.0 s | 57.0 s |
| Codex Luna Fast | fr | 5 / 6 | 48.8 s | 78.8 s |
| OpenCode2 GLM Flash | files | 4 / 6 | 61.7 s | 136.7 s |
| OpenCode2 GLM Flash | fr | 2 / 6 | 77.8 s | 377.9 s |
| OpenCode2 Kimi K3 | files | 4 / 6 | 61.0 s | 136.0 s |
| OpenCode2 Kimi K3 | fr | 4 / 6 | 109.5 s | 184.5 s |

Time includes the complete local attempt: CLI startup, model replies, tool calls, checks and patch capture.
Failed attempts stay in the totals. A valid patch delivered after the deadline still counts as a failed attempt.

| Agent | Pairs accepted in both setups | Ordinary mean | fr mean | Change with fr |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | 5 | 49.0 s | 48.8 s | -0.5% |
| OpenCode2 GLM Flash | 2 | 48.8 s | 77.8 s | 59.7% |
| OpenCode2 Kimi K3 | 4 | 61.0 s | 109.5 s | 79.5% |

Positive percentages mean fr took longer on matching successful attempts. This small comparison measures these
apps, models and requests; it does not establish general speed or money savings. Account charges are unknown.
Reported token categories retain each CLI’s meaning. Missing counters remain unknown.

| Attempt | Result | Elapsed seconds | Reported tokens | fr contribution or failure |
| --- | --- | --- | --- | --- |
| codex-luna-fast-access-rust-files-1 | passed | 29.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 99712, "input_tokens": 115341, "output_tokens": 1579, "reasoning_output_tokens": 672} | No fr contribution credited. |
| codex-luna-fast-access-rust-fr-1 | passed | 24.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 81024, "input_tokens": 98526, "output_tokens": 1630, "reasoning_output_tokens": 741} | Checked the agent-written proof with fr and Lean before delivering its patch. |
| codex-luna-fast-page-count-fr-1 | passed | 71.5 | {"cache_write_input_tokens": 0, "cached_input_tokens": 362624, "input_tokens": 432035, "output_tokens": 5450, "reasoning_output_tokens": 2563} | Checked and repaired the agent-written tactics with fr, then used fr to write them into the unchanged theorem. |
| codex-luna-fast-page-count-files-1 | passed | 112.0 | {"cache_write_input_tokens": 0, "cached_input_tokens": 446464, "input_tokens": 517695, "output_tokens": 8765, "reasoning_output_tokens": 4484} | No fr contribution credited. |
| codex-luna-fast-fits-window-files-1 | passed | 28.3 | {"cache_write_input_tokens": 0, "cached_input_tokens": 77568, "input_tokens": 88812, "output_tokens": 1614, "reasoning_output_tokens": 830} | No fr contribution credited. |
| codex-luna-fast-fits-window-fr-1 | passed | 56.4 | {"cache_write_input_tokens": 0, "cached_input_tokens": 148608, "input_tokens": 193550, "output_tokens": 3108, "reasoning_output_tokens": 1594} | Checked and repaired the agent-written tactics with fr, then used fr to write them into the unchanged theorem. |
| opencode2-glm-flash-access-rust-fr-1 | passed | 62.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-access-rust-files-1 | passed | 49.3 | {"cache": {"read": 32576, "write": 0}, "input": 9257, "output": 615, "reasoning": 258} | No fr contribution credited. |
| opencode2-glm-flash-page-count-files-1 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-fits-window-fr-1 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-fits-window-files-1 | passed | 89.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-files-1 | passed | 80.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-fr-1 | passed | 111.0 | Unknown or incomplete | Ran the declared Lean package check through fr using its reviewed configuration fingerprint; the check passed. |
| opencode2-kimi-k3-page-count-fr-1 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-page-count-files-1 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-fits-window-files-1 | passed | 42.7 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-fits-window-fr-1 | passed | 130.9 | Unknown or incomplete | Ran the declared Lean package check through fr using its reviewed configuration fingerprint; the check passed. |
| codex-luna-fast-access-rust-fr-2 | passed | 59.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 101248, "input_tokens": 147797, "output_tokens": 1659, "reasoning_output_tokens": 668} | Checked and repaired the agent-written tactics with fr, then used fr to write them into the unchanged theorem. |
| codex-luna-fast-access-rust-files-2 | passed | 25.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 34432, "input_tokens": 73336, "output_tokens": 1346, "reasoning_output_tokens": 517} | No fr contribution credited. |
| codex-luna-fast-page-count-files-2 | passed | 96.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 411264, "input_tokens": 479042, "output_tokens": 7953, "reasoning_output_tokens": 3985} | No fr contribution credited. |
| codex-luna-fast-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | The deadline expired after fr rejected the proof fragment; a patch was submitted. |
| codex-luna-fast-fits-window-fr-2 | passed | 31.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 98304, "input_tokens": 134489, "output_tokens": 1752, "reasoning_output_tokens": 820} | Checked and repaired the agent-written tactics with fr, then used fr to write them into the unchanged theorem. |
| codex-luna-fast-fits-window-files-2 | passed | 49.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 70016, "input_tokens": 104972, "output_tokens": 2239, "reasoning_output_tokens": 1256} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-files-2 | passed | 48.3 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-access-rust-fr-2 | passed | 92.9 | Unknown or incomplete | Checked and repaired the agent-written tactics with fr, then used fr to write them into the unchanged theorem. |
| opencode2-glm-flash-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-page-count-files-2 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-fits-window-files-2 | passed | 59.4 | {"cache": {"read": 35584, "write": 0}, "input": 8396, "output": 688, "reasoning": 623} | No fr contribution credited. |
| opencode2-glm-flash-fits-window-fr-2 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-access-rust-fr-2 | passed | 88.0 | {"cache": {"read": 107008, "write": 0}, "input": 7602, "output": 1392, "reasoning": 533} | Checked and repaired the agent-written tactics with fr, then used fr to write them into the unchanged theorem. |
| opencode2-kimi-k3-access-rust-files-2 | passed | 39.0 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-page-count-files-2 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-page-count-fr-2 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-fits-window-fr-2 | passed | 108.1 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-fits-window-files-2 | passed | 81.5 | Unknown or incomplete | No fr contribution credited. |

The published files contain submitted patches and this summary. Raw prompts, CLI transcripts, source archives
and account data stay local. The summary records the selected model settings, tool versions, checker identity,
source hashes, collection limits and approved axioms; failures and timeouts are retained.
