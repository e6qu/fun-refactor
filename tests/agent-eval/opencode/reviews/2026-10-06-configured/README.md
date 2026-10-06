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

The executable plan was committed before the calls. Its SHA-256 is
`5ba0b9e9778e695af36df053d589bea00bda241c0c5478a7a7ee3f70438e55e8`.
The [report](report.json) replays every retained artifact against that plan.

| Question | Kimi K3 | GLM 5.3 Flash |
| --- | --- | --- |
| One literal value with interpolation disabled | Completed, no finding | Completed, no finding |
| Duplicate values through a one-shot iterator | Completed, one unverified identity-check finding | Stopped at the memory limit |
| Default after rejecting every site-path entry | Not started | Not started |

The fourth call exceeded the unchanged 768-MiB sampled aggregate RSS limit: 808,255,488 bytes
after 4.43 seconds. The capture monitor killed it; the outer 1-GiB workload limit was not reached.
The [operator stop](stop.json) ends this collection at the first resource limit, as required by the
workstation policy. This is stricter than the frozen two-consecutive-failure rule, which did not
trigger. Do not resume the last two cells, retry attempted cells or raise limits.

The failed call retained one assistant message but no completed step or usage report. Missing
usage is unknown cost, not zero cost. All three completed reviews made zero fr calls. GLM read
additional dotenv source through six ordinary calls; both Kimi reviews used the supplied packet.
Client usage counters do not establish provider billing or complete context accounting. These
source reviews do not establish an agent efficiency advantage or acceptance of the whole tasks.

Kimi correctly identified that checking identity against *any* original item cannot establish
position-by-position identity. Its concrete example assumes duplicate string literals are distinct
objects; that premise is not established. The new `duplicate-final-identity` grader case instead
constructs two equal, explicitly distinct `Version` objects and also checks legitimate repeated
references to the same object. The `reuse-equal-output-object` control reuses the first equal object.
GitHub must verify that this same incorrect repair passes the frozen seven-case grader and public
example but fails the new case, while the reference continues to pass. Until that execution is
retained, the broader counterexample remains unverified; the original model finding stays unchanged.

The [hosted protocol check](https://github.com/e6qu/fun-refactor/actions/runs/37473198454)
passed in 1m53s, including the real OpenCode client with a scripted configured provider. It requires
no model-provider secret. It verifies the transport and submission checks, not model behavior.
