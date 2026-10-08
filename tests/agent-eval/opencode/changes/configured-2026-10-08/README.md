# Four packaging fix attempts with configured OpenCode models

This pilot asks Kimi K3 and GLM 5.3 Flash to fix the same prerelease filtering bug
in pinned packaging source. Each model gets one attempt with ordinary source tools
and one with those tools plus fr. The order is reversed for the second model.

The task was [admitted after scoped review](../../reviews/2026-10-07-packaging-scoped/admission.json).
One review remains failed. This is a familiar task with finite behavior checks;
it cannot establish general correctness or an efficiency advantage for fr.

The collection stopped after its first attempt hit the 20-CPU-second limit. Kimi
made five ordinary tool calls, including one accepted source replacement, but did
not finish a terminal submission. That partial edit is not a submitted repair and
is not graded. The other three planned attempts remain unstarted. No cell is retried.

| Attempt | Outcome | Tool calls | Result bytes | Elapsed | Peak RSS |
| --- | --- | ---: | ---: | ---: | ---: |
| Kimi, ordinary tools | CPU limit; no submission | 5 | 8,645 | 63.11 s | 417.23 MiB |
| Kimi, ordinary tools plus fr | Not started | Unknown | Unknown | Unknown | Unknown |
| GLM, ordinary tools plus fr | Not started | Unknown | Unknown | Unknown | Unknown |
| GLM, ordinary tools | Not started | Unknown | Unknown | Unknown | Unknown |

The plan was committed at `c0980440` before the live call. Its SHA-256 is
`6444b05e4c7546a0d4876760a79e525b637d35bbf8f9f32cd79696a3e530095a`.
Both [hosted controls](https://github.com/e6qu/fun-refactor/actions/runs/37768485645)
passed. The workstation control completed at 298.58 MiB peak RSS, 9.78 CPU seconds
and 23.95 seconds elapsed. Those short scripted controls did not predict the live
attempt's CPU demand. The live capture stopped at 20.03 sampled CPU seconds,
with 5.18 MiB disk growth; memory, disk and transcript limits were not reached.

Every attempt keeps the existing 120-second wall, 20-second CPU, 768-MiB RSS,
16-MiB disk-growth, 1-MiB transcript, 12-response and 24-tool-call limits. Model
profiles use the configured providers, low reasoning, 32,768 context and 2,048
output tokens. The client disables JIT and retains the admitted small-heap settings.
Each local command must also run through the workstation resource guard.

`stop.json` permanently closes this collection. The driver required matching hosted
and workstation evidence and committed plan, inputs and runner before the live call.
Collection permits no retries, stops on the first resource failure or two consecutive
capture failures, and retains unstarted cells as such. Candidate code runs only in
separate GitHub grading; this collection has no eligible submission to execute.

`public-baseline.zip` is the original hosted control artifact. Only its unchanged
candidate's public command and failure enter model prompts. Neither the private
grader nor reference edits are available through model tools. The frozen runtime
is retained so reports and grading can be replayed after later code changes.

The retained prefix has four completed usage records: 4,974 input, 912 output,
597 reasoning and 5,120 cache-read tokens as reported by the client. Reasoning must
not be added to output. The unfinished response and provider accounting remain
unverified. Reported cost zero does not establish zero dollar cost.

The report distinguishes collection success from behavior success and retains
failed work in observed costs. Tool-result bytes are not complete model context.
There is no completed comparison, no observed fr use and no efficiency result.
Next, attribute CPU use with a representative scripted workload on GitHub before
considering a new collection. Keep the limits and this stopped collection unchanged.
