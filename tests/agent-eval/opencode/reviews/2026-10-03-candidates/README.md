# Candidate grader review attempts

Six reviews were planned: three tasks with each configured OpenCode model. Two were attempted;
both stopped at the shared 120-second wall deadline without a completed review. The other four
were not started. No review findings, independent acceptance or efficiency result are claimed.

| Task | Model | Result | Wall seconds | Sampled CPU seconds | Peak aggregate RSS |
| --- | --- | --- | --- | --- | --- |
| dotenv-alternate | kimi-code-plan-global/k3 | Wall timeout | 120.071 | 4.51 | 685,457,408 bytes |
| dotenv-alternate | zai-coding-plan/glm-5.3-flash | Wall timeout | 120.042 | 5.35 | 673,480,704 bytes |
| packaging-prerelease | Both models | Not started | — | — | — |
| platformdirs-xdg | Both models | Not started | — | — | — |

The collector and inputs were committed as `1102e300` before the first call. `plan.json` freezes
their hashes, the published source revision, model IDs, executable identity and process budgets.
The prompts contain requirements and public/private grader definitions, without source snapshots,
reference repairs or mutation controls. All prompt data was reconstructed from public files at
the recorded revision; `public-input-provenance.json` retains hashes and links.

Both calls used OpenCode 1.18.34 in isolated directories with tools denied. Version detection,
review and export shared 120 wall seconds and 20 sampled CPU seconds, with a 768 MiB RSS limit.
The workstation guard remained active. Neither candidate code nor graders executed locally.
After both models timed out on the first task, collection stopped without retries or larger limits.

Each original attempt retains its streams, process measurements, failure and file manifest.
Separate bounded session exports in `diagnostics/` preserve the requested model and message
identities. They show unfinished assistant messages with no completed response or reported error.
Partial reasoning is not a completed review and was not used as review findings. These exports
do not establish why the calls timed out. Their combined sampled CPU use was 1.11 seconds,
separate from the 9.86 seconds recorded by collection. Complete token use and provider billing
are unknown; zero counters on unfinished messages do not establish zero cost.

The six new [grader controls](../../candidates/README.md) are task-author counterexamples.
GitHub must demonstrate that each passes its frozen original grader and public example, then
fails its named new private case. This tests specific omissions; it does not make the review
independent or show that the graders are complete. Independent task/grader review remains open.

`evidence-sha256.json` inventories the frozen collection and diagnostic files. The offline
`tools/agent_eval/test_candidate_reviews.py` check verifies those bytes, input provenance,
failed-attempt counts and unfinished session identities without model calls. Hash consistency
does not authenticate the evidence publisher. Run local checks through the workstation guard.
