# CI latency and coverage

PR checks have a 15-minute job deadline. Native tests run on six GitHub runners;
the Python SDK runs on four more. Each runner executes tests serially with one
Lean worker. Local builds and resource limits do not change.

The last measured serial baseline, [run 36592893805](https://github.com/e6qu/fun-refactor/actions/runs/36592893805),
spent 54 minutes in `cargo test --all-targets`. Python accounted for 18 minutes,
formal kernels for 6.4 minutes, author commands for 5 minutes, and project commands
for 3.8 minutes. `tools/ci-test-durations.json` retains those suite measurements.
Native scheduling uses longest estimated duration first. Unknown targets receive
a conservative ten-second estimate and enter the partition automatically.

`tools/ci-shards.py` discovers native targets through Cargo metadata. It includes
library, binary, integration, example and benchmark targets. The native Python
SDK wrapper delegates its pytest invocation to the four SDK jobs; all its other
Rust tests still run. Pytest collects the entire suite in each job and assigns
complete node IDs, including parameters, by a stable hash. New cases enter the
partition automatically.

Every successful shard uploads its complete inventory, assignments, capability
log and advertised matrix. The final `check (default)` gate requires all shards
and static checks to pass, verifies identical inventories and complete, disjoint
assignments, then checks combined capability coverage. Missing artifacts, duplicate
assignments, differing matrices and uncovered capabilities fail the gate.

Formatting, strict Clippy, the focused VFS regressions, Lean kernel checks, Python
type checking and prose checks run in the static job. Browser feature checks and
the playground remain separate. `tools/check.sh default` still runs the whole
native gate serially when appropriate; local resource policy takes precedence.
The post-merge deep audit retains its existing separate workflow.

The runtime target excludes runner queue delays. Inspect job timestamps and shard
logs after changes to suite size; update measured weights or split work before a
job reaches its deadline. A deadline failure must not cause tests to be removed.

The pinned Zig archive now has one checksum-verified cache shared by native jobs and the deep audit.
A five-minute preparation job fills that cache before the PR shards start. Every restore rechecks the
archive digest. Interrupted range downloads resume verified bytes with five bounded 45-second attempts;
they retain the existing five-minute installation limit. This addresses the download timeout in
[main run 36787051550](https://github.com/e6qu/fun-refactor/actions/runs/36787051550).

The study job also exercises isolated graders and agent command containers with an image whose local
content ID it records after pulling. Offline providers drive a complete command/edit/grade attempt,
including a synthetic executable that tests the fr mount. This fixture does not measure model quality
or real fr effectiveness. Live runs must freeze their own image digest, binary and independent cases.
The same attempt reads source before and after the isolated edit and verifies that the report keeps
command-internal reads unknown. Offline cases cover UTF-8 paging, stale continuations, overlap across
agents, retained failures and rejection of altered disclosure counters.
The study job also prepares temporary systemd slices on its disposable Linux runner. It verifies
shared command/grader accounting, CPU-budget termination of detached descendants, and kernel memory
limits. Those tests remove their dedicated units and containers; workstation checks use fake counters
and never change local cgroups or start Docker.
The same study job runs the OpenCode rehearsal's offline parser, evidence and private-grader tests.
It does not launch OpenCode or contact model providers; live local attempts use the workstation guard.
