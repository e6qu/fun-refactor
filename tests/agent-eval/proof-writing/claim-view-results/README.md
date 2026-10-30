# M3: writing three fixed Lean proofs

All 36 scheduled local CLI attempts were independently checked: 27 passed and 9 failed.

Still missing accepted proofs using fr:

- OpenCode2 GLM Flash: Access for staff or the owner.
- OpenCode2 GLM Flash: Safe page counts.
- OpenCode2 GLM Flash: A range fits within its limit.
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
| Codex Luna Fast | files | 6 / 6 | 40.1 s | 40.1 s |
| Codex Luna Fast | fr | 6 / 6 | 53.1 s | 53.1 s |
| OpenCode2 GLM Flash | files | 4 / 6 | 43.2 s | 118.2 s |
| OpenCode2 GLM Flash | fr | 3 / 6 | 42.1 s | 192.1 s |
| OpenCode2 Kimi K3 | files | 4 / 6 | 65.7 s | 140.7 s |
| OpenCode2 Kimi K3 | fr | 4 / 6 | 71.3 s | 146.4 s |

Time includes the complete local attempt: CLI startup, model replies, tool calls, checks and patch capture.
Failed attempts stay in the totals. A valid patch delivered after the deadline still counts as a failed attempt.

| Agent | Pairs accepted in both setups | Ordinary mean | fr mean | Change with fr |
| --- | --- | --- | --- | --- |
| Codex Luna Fast | 6 | 40.1 s | 53.1 s | 32.5% |
| OpenCode2 GLM Flash | 3 | 42.4 s | 42.1 s | -0.7% |
| OpenCode2 Kimi K3 | 4 | 65.7 s | 71.3 s | 8.6% |

Positive percentages mean fr took longer on matching successful attempts. This small comparison measures these
apps, models and requests; it does not establish general speed or money savings. Account charges are unknown.
Reported token categories retain each CLI’s meaning. Missing counters remain unknown.

| Attempt | Result | Elapsed seconds | Reported tokens | fr contribution or failure |
| --- | --- | --- | --- | --- |
| codex-luna-fast-access-rust-files-1 | passed | 20.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 60160, "input_tokens": 74806, "output_tokens": 1359, "reasoning_output_tokens": 654} | No fr contribution credited. |
| codex-luna-fast-access-rust-fr-1 | passed | 31.8 | {"cache_write_input_tokens": 0, "cached_input_tokens": 142848, "input_tokens": 166182, "output_tokens": 2136, "reasoning_output_tokens": 1100} | Inspected the fixed claim, checked the draft and saved its proof with fr. |
| codex-luna-fast-page-count-fr-1 | passed | 64.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 309504, "input_tokens": 359769, "output_tokens": 5798, "reasoning_output_tokens": 3387} | Inspected the claim parts and used repeated fr proof checks to repair the draft before saving it. |
| codex-luna-fast-page-count-files-1 | passed | 85.4 | {"cache_write_input_tokens": 0, "cached_input_tokens": 296064, "input_tokens": 351881, "output_tokens": 6473, "reasoning_output_tokens": 3498} | No fr contribution credited. |
| codex-luna-fast-fits-window-files-1 | passed | 23.0 | {"cache_write_input_tokens": 0, "cached_input_tokens": 90112, "input_tokens": 99600, "output_tokens": 1614, "reasoning_output_tokens": 741} | No fr contribution credited. |
| codex-luna-fast-fits-window-fr-1 | passed | 44.1 | {"cache_write_input_tokens": 0, "cached_input_tokens": 145536, "input_tokens": 171146, "output_tokens": 2270, "reasoning_output_tokens": 999} | Inspected the claim, checked the draft and saved its proof with fr. |
| opencode2-glm-flash-access-rust-fr-1 | passed | 44.5 | Unknown or incomplete | No useful fr step observed; its check pipeline returned no output. |
| opencode2-glm-flash-access-rust-files-1 | passed | 35.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-page-count-files-1 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after failed Lean builds and library searches. |
| opencode2-glm-flash-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | Used fr to locate the goal and check a draft; diagnostics identified an incorrect assumption split.; Timed out without a patch after a rejected draft check at about 121 seconds. |
| opencode2-glm-flash-fits-window-fr-1 | passed | 41.2 | Unknown or incomplete | No fr command observed; the proof was written and built with ordinary tools. |
| opencode2-glm-flash-fits-window-files-1 | passed | 46.5 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-files-1 | passed | 29.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-access-rust-fr-1 | passed | 54.5 | Unknown or incomplete | Inspected the claim, checked and saved the proof, then verified its Lean package with fr. |
| opencode2-kimi-k3-page-count-fr-1 | failed | 150.0 | Unknown or incomplete | Timed out without a patch; no fr command observed, and its first Lean build began at about 128 seconds. |
| opencode2-kimi-k3-page-count-files-1 | failed | 150.1 | Unknown or incomplete | Timed out without a patch after two failed Lean builds. |
| opencode2-kimi-k3-fits-window-files-1 | passed | 54.3 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-fits-window-fr-1 | passed | 68.5 | Unknown or incomplete | Inspected the claim parts and used fr to check and save the proof. |
| codex-luna-fast-access-rust-fr-2 | passed | 29.6 | {"cache_write_input_tokens": 0, "cached_input_tokens": 94976, "input_tokens": 130056, "output_tokens": 1868, "reasoning_output_tokens": 770} | Inspected the fixed claim, checked the draft and saved its proof with fr. |
| codex-luna-fast-access-rust-files-2 | passed | 19.3 | {"cache_write_input_tokens": 0, "cached_input_tokens": 64640, "input_tokens": 74556, "output_tokens": 1118, "reasoning_output_tokens": 472} | No fr contribution credited. |
| codex-luna-fast-page-count-files-2 | passed | 56.5 | {"cache_write_input_tokens": 0, "cached_input_tokens": 265344, "input_tokens": 292719, "output_tokens": 5366, "reasoning_output_tokens": 2526} | No fr contribution credited. |
| codex-luna-fast-page-count-fr-2 | passed | 85.9 | {"cache_write_input_tokens": 0, "cached_input_tokens": 402048, "input_tokens": 455605, "output_tokens": 5989, "reasoning_output_tokens": 3183} | Inspected the claim parts and used repeated fr proof checks to repair the delivered proof. |
| codex-luna-fast-fits-window-fr-2 | passed | 63.4 | {"cache_write_input_tokens": 0, "cached_input_tokens": 247552, "input_tokens": 266680, "output_tokens": 5074, "reasoning_output_tokens": 3204} | Used fr diagnostics to repair rejected drafts, then checked and saved the proof. |
| codex-luna-fast-fits-window-files-2 | passed | 35.6 | {"cache_write_input_tokens": 0, "cached_input_tokens": 115456, "input_tokens": 139010, "output_tokens": 2343, "reasoning_output_tokens": 917} | No fr contribution credited. |
| opencode2-glm-flash-access-rust-files-2 | passed | 44.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-access-rust-fr-2 | passed | 40.4 | Unknown or incomplete | No fr command observed; the proof was written and built with ordinary tools. |
| opencode2-glm-flash-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after a failed Lean build; no fr command observed. |
| opencode2-glm-flash-page-count-files-2 | failed | 150.0 | Unknown or incomplete | Timed out without a patch after attempting the fixed page-count proof. |
| opencode2-glm-flash-fits-window-files-2 | passed | 45.8 | Unknown or incomplete | No fr contribution credited. |
| opencode2-glm-flash-fits-window-fr-2 | failed | 150.0 | Unknown or incomplete | Used fr to check rejected drafts; diagnostics exposed Boolean comparisons that had not become arithmetic assumptions.; Timed out without a patch after wrong target guesses, rejected drafts and failed Boolean-to-arithmetic conversions. |
| opencode2-kimi-k3-access-rust-fr-2 | passed | 113.7 | Unknown or incomplete | No fr command observed; the proof was written and built with ordinary tools. |
| opencode2-kimi-k3-access-rust-files-2 | passed | 66.7 | Unknown or incomplete | No fr contribution credited. |
| opencode2-kimi-k3-page-count-files-2 | failed | 150.1 | Unknown or incomplete | Timed out without a patch after attempting the fixed page-count proof. |
| opencode2-kimi-k3-page-count-fr-2 | failed | 150.0 | Unknown or incomplete | Inspected the claim parts and used fr to check a draft, which Lean rejected.; Timed out without a patch after a rejected draft check. |
| opencode2-kimi-k3-fits-window-fr-2 | passed | 48.7 | Unknown or incomplete | Used fr to verify the Lean package containing the delivered proof. |
| opencode2-kimi-k3-fits-window-files-2 | passed | 112.0 | Unknown or incomplete | No fr contribution credited. |

The published files contain submitted patches and this summary. Raw prompts, CLI transcripts, source archives
and account data stay local. The summary records the selected model settings, tool versions, checker identity,
source hashes, collection limits and approved axioms; failures and timeouts are retained.

The [behavior review](../behavior-review.md) examines GLM and Kimi commands and timing.
