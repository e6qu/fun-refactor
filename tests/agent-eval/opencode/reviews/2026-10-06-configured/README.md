# Source reviews with the configured OpenCode client

This collection uses OpenCode's existing provider access. The evaluator does not read, copy or
upload authentication files. GitHub operations use the existing `gh` authentication.

The design carries forward the three unattempted questions from the
[prepared terminal design](../2026-10-06-terminal-design/README.md). It explicitly selects
configured-client access for `kimi-code-plan-global/k3` and `zai-coding-plan/glm-5.3-flash`.
Provider endpoints and credentials are resolved by OpenCode and are not frozen or verified here.
The plan binds requested model identities, configured context/output caps, source, tools, runtime
and both executable hashes. Exported assistant identities must match the requested model.

The source archive, reference repairs, graders and questions are unchanged. Previous stopped
collections remain stopped. These reviews do not execute candidate code or establish whole-task
acceptance. Any finding needs separate behavior verification before changing a grader.

Each local attempt runs alone through `fr-local-guard.py`, with the existing 120-second capture
deadline, 20-second sampled CPU limit, 768-MiB sampled RSS limit and 16-MiB capture-directory
growth limit. The guard also limits the whole local workload and preserves disk headroom.
OpenCode manages its normal client data; the capture-directory measurement does not establish
complete cache growth or total system resource usage. No local builds are required.

The fresh executable plan and attempt records will be retained here before any outcomes are claimed.
Two consecutive failures stop the remaining cells. Do not retry attempted cells or raise limits.
Client usage counters do not establish provider billing or complete context accounting.
