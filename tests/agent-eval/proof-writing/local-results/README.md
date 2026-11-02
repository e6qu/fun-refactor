# M3: writing three fixed Lean proofs

All 36 scheduled local CLI attempts were independently checked: 28 passed and 8 failed.

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
| Codex Luna Fast | files | 6 / 6 | 45.6 s | 45.6 s |
| Codex Luna Fast | fr | 6 / 6 | 45.1 s | 45.1 s |
| OpenCode2 GLM Flash | files | 4 / 6 | 42.3 s | 117.3 s |
| OpenCode2 GLM Flash | fr | 4 / 6 | 60.8 s | 135.8 s |
| OpenCode2 Kimi K3 | files | 4 / 6 | 55.1 s | 130.1 s |
| OpenCode2 Kimi K3 | fr | 4 / 6 | 70.6 s | 145.6 s |

Time includes the complete local attempt: CLI startup, model replies, tool calls, checks and patch capture.
Failed attempts stay in the totals. A valid patch delivered after the deadline still counts as a failed attempt.

| Agent | Pairs accepted in both setups | Ordinary mean | fr mean | Change with fr |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | 6 | 45.6 s | 45.1 s | -1.0% |
| OpenCode2 GLM Flash | 4 | 42.3 s | 60.8 s | 43.8% |
| OpenCode2 Kimi K3 | 4 | 55.1 s | 70.6 s | 28.2% |

Positive percentages mean fr took longer on matching successful attempts. This small comparison measures these
apps, models and requests; it does not establish general speed or money savings. Account charges are unknown.
Reported token categories retain each CLI’s meaning. Missing counters remain unknown.

| Attempt | Result | Elapsed seconds | Reported tokens | fr contribution or failure |
| --- | --- | --- | --- | --- |
| codex-luna-fast-access-rust-files-1 | passed | 22.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 59008, "input_tokens": 85560, "output_tokens": 1143, "reasoning_output_tokens": 501} | No fr contribution credited. |
| codex-luna-fast-access-rust-fr-1 | passed | 26.0 | {"cache_write_input_tokens": 0, "cached_input_tokens": 66688, "input_tokens": 95023, "output_tokens": 1370, "reasoning_output_tokens": 520} | Checked a draft successfully and saved its proof with fr. |
| codex-luna-fast-page-count-fr-1 | passed | 72.6 | {"cache_write_input_tokens": 0, "cached_input_tokens": 369792, "input_tokens": 425428, "output_tokens": 4608, "reasoning_output_tokens": 1926} | Checked a draft successfully and saved its proof with fr. |
| codex-luna-fast-page-count-files-1 | passed | 92.2 | {"cache_write_input_tokens": 0, "cached_input_tokens": 355968, "input_tokens": 417426, "output_tokens": 7839, "reasoning_output_tokens": 4298} | No fr contribution credited. |
| codex-luna-fast-fits-window-files-1 | passed | 25.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 90496, "input_tokens": 103636, "output_tokens": 1433, "reasoning_output_tokens": 524} | No fr contribution credited. |
| codex-luna-fast-fits-window-fr-1 | passed | 36.7 | {"cache_write_input_tokens": 0, "cached_input_tokens": 120064, "input_tokens": 130687, "output_tokens": 1812, "reasoning_output_tokens": 931} | Checked a draft successfully and saved its proof with fr. |
| opencode2-glm-flash-access-rust-fr-1 | passed | 47.8 | Unknown or incomplete | Located the goal, checked its draft successfully and saved the proof with fr. |
| opencode2-glm-flash-access-rust-files-1 | passed | 38.0 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-page-count-files-1 | failed | 150.1 | Unknown or incomplete | Timed out without a patch after repeated failed builds, syntax errors and library searches. |
| opencode2-glm-flash-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after reading the claim and repeated library searches; no draft check observed. |
| opencode2-glm-flash-fits-window-fr-1 | passed | 97.8 | Unknown or incomplete | Used fr to check the draft successfully, save its proof and verify the Lean package. |
| opencode2-glm-flash-fits-window-files-1 | passed | 53.1 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-files-1 | passed | 41.5 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-fr-1 | passed | 66.5 | Unknown or incomplete | Located the goal, checked its draft successfully and saved the proof with fr. |
| opencode2-kimi-k3-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after goal discovery and library searches; no draft check observed. |
| opencode2-kimi-k3-page-count-files-1 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after failed builds using unresolved inequality names. |
| opencode2-kimi-k3-fits-window-files-1 | passed | 56.4 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-fits-window-fr-1 | passed | 69.7 | Unknown or incomplete | Checked a draft successfully and saved its proof with fr. |
| codex-luna-fast-access-rust-fr-2 | passed | 31.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 98176, "input_tokens": 133527, "output_tokens": 1680, "reasoning_output_tokens": 709} | Checked a draft successfully and saved its proof with fr. |
| codex-luna-fast-access-rust-files-2 | passed | 22.6 | {"cache_write_input_tokens": 0, "cached_input_tokens": 59520, "input_tokens": 86523, "output_tokens": 1124, "reasoning_output_tokens": 439} | No fr contribution credited. |
| codex-luna-fast-page-count-files-2 | passed | 86.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 384128, "input_tokens": 433403, "output_tokens": 6447, "reasoning_output_tokens": 3299} | No fr contribution credited. |
| codex-luna-fast-page-count-fr-2 | passed | 72.5 | {"cache_write_input_tokens": 0, "cached_input_tokens": 309376, "input_tokens": 346037, "output_tokens": 5160, "reasoning_output_tokens": 2877} | Checked a draft successfully and saved its proof with fr. |
| codex-luna-fast-fits-window-fr-2 | passed | 31.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 81920, "input_tokens": 117071, "output_tokens": 1660, "reasoning_output_tokens": 746} | Checked a draft successfully and saved its proof with fr. |
| codex-luna-fast-fits-window-files-2 | passed | 24.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 106240, "input_tokens": 116382, "output_tokens": 1604, "reasoning_output_tokens": 591} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-files-2 | passed | 37.6 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-access-rust-fr-2 | passed | 48.3 | Unknown or incomplete | Checked a draft successfully and saved its proof with fr. |
| opencode2-glm-flash-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after library searches and two failed builds; no fr command observed. |
| opencode2-glm-flash-page-count-files-2 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after reading the model and searching integer-division facts; no completed build observed. |
| opencode2-glm-flash-fits-window-files-2 | passed | 40.4 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-fits-window-fr-2 | passed | 49.2 | Unknown or incomplete | Checked a draft successfully and saved its proof with fr. |
| opencode2-kimi-k3-access-rust-fr-2 | passed | 64.2 | {"cache": {"read": 55808, "write": 0}, "input": 4297, "output": 824, "reasoning": 62} | Checked a draft successfully and saved its proof with fr. |
| opencode2-kimi-k3-access-rust-files-2 | passed | 42.9 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-page-count-files-2 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after library searches; no completed build observed. |
| opencode2-kimi-k3-page-count-fr-2 | failed | 150.1 | Unknown or incomplete | Inspected the claim and checked two drafts; Lean rejected both.; Timed out without a patch after reading the claim and two rejected draft checks. |
| opencode2-kimi-k3-fits-window-fr-2 | passed | 81.9 | Unknown or incomplete | Checked a draft successfully and saved its proof with fr. |
| opencode2-kimi-k3-fits-window-files-2 | passed | 79.6 | Unknown or incomplete | No fr contribution credited. |

The published files contain submitted patches and this summary. Raw prompts, CLI transcripts, source archives
and account data stay local. The summary records the selected model settings, tool versions, checker identity,
source hashes, collection limits and approved axioms; failures and timeouts are retained.

The [behavior review](../behavior-review.md) examines GLM and Kimi commands and timing.
