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
| Codex Luna Fast | files | 6 / 6 | 63.2 s | 63.2 s |
| Codex Luna Fast | fr | 4 / 6 | 46.4 s | 114.2 s |
| OpenCode2 GLM Flash | files | 4 / 6 | 78.2 s | 153.2 s |
| OpenCode2 GLM Flash | fr | 4 / 6 | 88.8 s | 163.8 s |
| OpenCode2 Kimi K3 | files | 4 / 6 | 60.7 s | 135.7 s |
| OpenCode2 Kimi K3 | fr | 4 / 6 | 78.9 s | 153.9 s |

Time includes the complete local attempt: CLI startup, model replies, tool calls, checks and patch capture.
Failed attempts stay in the totals. A valid patch delivered after the deadline still counts as a failed attempt.

| Agent | Pairs accepted in both setups | Ordinary mean | fr mean | Change with fr |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | 4 | 33.1 s | 46.4 s | 40.4% |
| OpenCode2 GLM Flash | 4 | 78.2 s | 88.8 s | 13.6% |
| OpenCode2 Kimi K3 | 4 | 60.7 s | 78.9 s | 30.1% |

Positive percentages mean fr took longer on matching successful attempts. This small comparison measures these
apps, models and requests; it does not establish general speed or money savings. Account charges are unknown.
Reported token categories retain each CLI’s meaning. Missing counters remain unknown.

| Attempt | Result | Elapsed seconds | Reported tokens | fr contribution or failure |
| --- | --- | --- | --- | --- |
| codex-luna-fast-access-rust-files-1 | passed | 29.0 | {"cache_write_input_tokens": 0, "cached_input_tokens": 93568, "input_tokens": 113342, "output_tokens": 1612, "reasoning_output_tokens": 622} | No fr contribution credited. |
| codex-luna-fast-access-rust-fr-1 | passed | 32.3 | {"cache_write_input_tokens": 0, "cached_input_tokens": 103680, "input_tokens": 113006, "output_tokens": 1856, "reasoning_output_tokens": 992} | Used fr to check the agent-written tactics with Lean and write them into the unchanged theorem. |
| codex-luna-fast-page-count-fr-1 | failed | 131.4 | {"cache_write_input_tokens": 0, "cached_input_tokens": 512768, "input_tokens": 590571, "output_tokens": 9038, "reasoning_output_tokens": 4424} | Submitted an empty patch after Lean rejected the proposed proof. |
| codex-luna-fast-page-count-files-1 | passed | 126.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 574848, "input_tokens": 616756, "output_tokens": 8368, "reasoning_output_tokens": 4126} | No fr contribution credited. |
| codex-luna-fast-fits-window-files-1 | passed | 49.3 | {"cache_write_input_tokens": 0, "cached_input_tokens": 117248, "input_tokens": 172011, "output_tokens": 2811, "reasoning_output_tokens": 1387} | No fr contribution credited. |
| codex-luna-fast-fits-window-fr-1 | passed | 41.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 119040, "input_tokens": 129065, "output_tokens": 2142, "reasoning_output_tokens": 990} | Checked the agent-written tactics with fr and Lean before delivering the proof patch. |
| opencode2-glm-flash-access-rust-fr-1 | passed | 109.0 | Unknown or incomplete | Used fr to check the agent-written tactics with Lean and write them into the unchanged theorem. |
| opencode2-glm-flash-access-rust-files-1 | passed | 84.5 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-page-count-files-1 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-page-count-fr-1 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-fits-window-fr-1 | passed | 103.3 | {"cache": {"read": 36672, "write": 0}, "input": 16305, "output": 856, "reasoning": 1470} | Used fr to check the agent-written tactics with Lean and write them into the unchanged theorem. |
| opencode2-glm-flash-fits-window-files-1 | passed | 67.3 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-files-1 | passed | 38.3 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-fr-1 | passed | 85.8 | Unknown or incomplete | Ran the declared Lean package check through fr with its reviewed configuration fingerprint; the check passed. |
| opencode2-kimi-k3-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-page-count-files-1 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-fits-window-files-1 | passed | 78.1 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-fits-window-fr-1 | passed | 77.9 | Unknown or incomplete | Used fr to check the agent-written tactics with Lean and write them into the unchanged theorem. |
| codex-luna-fast-access-rust-fr-2 | passed | 54.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 93824, "input_tokens": 147667, "output_tokens": 2721, "reasoning_output_tokens": 1527} | Used fr to check the agent-written tactics with Lean and write them into the unchanged theorem. |
| codex-luna-fast-access-rust-files-2 | passed | 24.2 | {"cache_write_input_tokens": 0, "cached_input_tokens": 79872, "input_tokens": 85711, "output_tokens": 1274, "reasoning_output_tokens": 525} | No fr contribution credited. |
| codex-luna-fast-page-count-files-2 | passed | 120.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 315008, "input_tokens": 387938, "output_tokens": 6582, "reasoning_output_tokens": 3537} | No fr contribution credited. |
| codex-luna-fast-page-count-fr-2 | failed | 139.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 490368, "input_tokens": 570116, "output_tokens": 8445, "reasoning_output_tokens": 4494} | Lean rejected the proposed proof; a patch was still submitted. |
| codex-luna-fast-fits-window-fr-2 | passed | 56.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 100864, "input_tokens": 131438, "output_tokens": 1825, "reasoning_output_tokens": 810} | Used fr to check the agent-written tactics with Lean and write them into the unchanged theorem. |
| codex-luna-fast-fits-window-files-2 | passed | 29.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 88704, "input_tokens": 115097, "output_tokens": 1757, "reasoning_output_tokens": 796} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-files-2 | passed | 74.3 | {"cache": {"read": 49472, "write": 0}, "input": 8527, "output": 776, "reasoning": 484} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-fr-2 | passed | 66.6 | Unknown or incomplete | Used fr to check the agent-written tactics with Lean and write them into the unchanged theorem. |
| opencode2-glm-flash-page-count-fr-2 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-page-count-files-2 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-glm-flash-fits-window-files-2 | passed | 86.6 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-fits-window-fr-2 | passed | 76.2 | Unknown or incomplete | Used fr to check the agent-written tactics with Lean and write them into the unchanged theorem. |
| opencode2-kimi-k3-access-rust-fr-2 | passed | 63.1 | Unknown or incomplete | Retrieved the access model and its Boolean argument types with fr before writing its proof. |
| opencode2-kimi-k3-access-rust-files-2 | passed | 36.5 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-page-count-files-2 | failed | 150.1 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | The 150-second deadline expired without a submitted patch. |
| opencode2-kimi-k3-fits-window-fr-2 | passed | 88.9 | Unknown or incomplete | Checked the agent-written tactics with fr and Lean before delivering the proof patch. |
| opencode2-kimi-k3-fits-window-files-2 | passed | 89.7 | Unknown or incomplete | No fr contribution credited. |

The published files contain submitted patches and this summary. Raw prompts, CLI transcripts, source archives
and account data stay local. The summary records the selected model settings, tool versions, checker identity,
source hashes, collection limits and approved axioms; failures and timeouts are retained.
