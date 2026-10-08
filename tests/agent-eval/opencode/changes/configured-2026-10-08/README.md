# Four packaging fix attempts with configured OpenCode models

This pilot asks Kimi K3 and GLM 5.3 Flash to fix the same prerelease filtering bug
in pinned packaging source. Each model gets one attempt with ordinary source tools
and one with those tools plus fr. The order is reversed for the second model.

The task was [admitted after scoped review](../../reviews/2026-10-07-packaging-scoped/admission.json).
One review remains failed. This is a familiar task with finite behavior checks;
it cannot establish general correctness or an efficiency advantage for fr.

Collection is waiting for fresh hosted controls and one guarded workstation control.
No live attempt has started. The eventual frozen plan will bind the exact source,
runtime, binaries, provider catalog, public feedback and admission evidence.

Every attempt keeps the existing 120-second wall, 20-second CPU, 768-MiB RSS,
16-MiB disk-growth, 1-MiB transcript, 12-response and 24-tool-call limits. Model
profiles use the configured providers, low reasoning, 32,768 context and 2,048
output tokens. The client disables JIT and retains the admitted small-heap settings.
Each local command must also run through the workstation resource guard.

`collect.py freeze` refuses without matching hosted and workstation evidence.
Commit the generated plan, inputs and runner before `collect.py collect --cell …
--confirm-agent-spend`. Collection permits no retries, stops on the first resource
failure or two consecutive capture failures, and retains unstarted cells as such.
The model can edit source, but candidate code runs only in separate GitHub grading.

`public-baseline.zip` is the original hosted control artifact. Only its unchanged
candidate's public command and failure enter model prompts. Neither the private
grader nor reference edits are available through model tools. The frozen runtime
is retained so reports and grading can be replayed after later code changes.

Final results will distinguish collection success from behavior success and retain
failed work in observed costs. Tool-result bytes are not complete model context.
Provider token counters keep their original meanings; dollar cost is unknown.
