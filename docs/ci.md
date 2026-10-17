# Run the complete checks without slowing the workstation

When you open a pull request, GitHub divides the native and Python tests across runners and checks that none are missing.
If a job times out, find what made it slow or stuck. Keep the existing deadline while fixing that cause.

CI means continuous integration: GitHub runs checks for a proposed change.
A workflow is a declared set of jobs. A shard is one assigned portion of the test suite.
The final gate checks that all required portions ran and passed; a passing small portion is insufficient.
Queue time is time waiting for a runner. Execution time starts when a runner begins work.
For a slow PR, inspect those times separately before deciding which operation needs repair.

## Save Python test progress without repeating earlier work

The Python collector runs small programs through Python itself and through `fr`, then saves both observations for comparison.
A case is one program with its chosen names and file locations.
The saving comparison uses 818 retained cases, including renamed copies and requests that the command deliberately refuses.

Previously, saving case 800 also encoded and rewrote the preceding 799 results.
The collector now appends the new result in JSON, a text format for structured data.
It closes the list again, leaving the completed cases readable after each append.
It writes and flushes each new row during collection, then saves the full report once after the final audit.
Flushing sends buffered text to the file. The final audit checks all observations and their recorded inputs.
The report's structure and checks stay the same.

For example, if a command times out while collecting case 801, the saved file still contains the first 800 completed results.
Its summary says the collection is incomplete, so a partial file cannot stand in for a passing audit.
If a result fails its check, collection continues and the final failed report records that error beside the observations.
The [writer tests](../tools/test-python-contract-writer.py) check these cases, Unicode text and interruption between completed appends.
These checks also run in the regular Python test gate.

Use the `python-report-writes` group in `refinement-evidence.yml` to compare saving methods on a GitHub runner.
It writes the same retained observations using the previous and current methods and requires identical final reports.
The comparison includes JSON encoding, writing, flushing and the final save.
Builds, execution of example programs, analysis and audits fall outside the timer.
Read its recorded environment and timings before making a speed claim about collecting results.
The collection and audit deadlines stay unchanged.

To rerun it, use an authenticated GitHub CLI from the repository root.
Replace `YOUR_BRANCH` with your working branch and `YOUR_NEW_NAME` with an unused dated name for the saved result.
The successful job commits its measurement to that branch.

```sh
gh workflow run refinement-evidence.yml --ref YOUR_BRANCH \
  -f group=python-report-writes -f prefix=YOUR_NEW_NAME
```

This replays the same retained observations as a fixed input for saving.
Use the `python-contract` group when you also need fresh analysis results.

## How the checks run

PR checks have a 15-minute job deadline. Native tests run on six GitHub runners;
the Python SDK runs on four more. Each runner executes tests serially with one
Lean worker. Local builds and resource limits do not change.

Go validation installs Go 1.27.2 from its official Linux archive and checks its
[published SHA-256 digest](https://go.dev/dl/?mode=json) before extraction.
This is the same release used by the successful audit jobs. Three other jobs could not start
because GitHub returned 404 for the `actions/setup-go` archive, despite the tag and commit existing.
The shared local action now installs Go directly. Both native checks and deep audits use it.
The installer has one source, verifies the archive before running it, and keeps the job deadlines.

Grading jobs use the public Docker registry mirror with an empty, job-local Docker
client configuration. This avoids the shared Hub pull limit and inherited login
path observed in failed runs. Original image digests and isolation checks remain
required. The setup needs no new credentials and changes only the GitHub runner.

The scripted client checks use the existing small, verified model catalog.
A catalog lists model settings, such as provider endpoints and supported tools.
Disabling downloads still loads the client's bundled catalog, which caused a [recorded memory stop](../tests/agent-eval/opencode/memory/2026-10-17-scripted-limit/README.md).
CI verifies the selected catalog before starting the client and passes its path through the isolated environment.
The fake provider still uses each test's explicit model configuration.
The 768 MiB process-group limit, deadlines and complete two-response checks stay unchanged.
These controls use scripted responses; live studies remain stopped.

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
complete node IDs, including parameters, by longest estimated duration first.
The recorded SDK timings come from [run 38670183873](https://github.com/e6qu/fun-refactor/actions/runs/38670183873).
Its name-based hash assignment put a report audit and several proof checks together, and one job
hit the deadline while other SDK groups finished much sooner. Measured slow cases now spread across
the same four runners. New cases receive a one-second estimate and enter the partition automatically.
Each group keeps pytest's collected execution order. All job deadlines remain unchanged.

Every successful shard uploads its complete inventory, assignments, capability
log and advertised matrix. The final `check (default)` gate requires all shards,
static checks, study checks and toolchain verification to pass. It verifies identical inventories and complete, disjoint
assignments, then checks combined capability coverage. Missing artifacts, duplicate
assignments, differing matrices and uncovered capabilities fail the gate.
The final gate runs after dependency failures but skips cancelled workflows. An `always()`
condition kept a superseded run queued after its test jobs cancelled, holding the concurrency slot
needed by the replacement run. See GitHub's [cancellation guidance](https://docs.github.com/en/actions/how-tos/troubleshoot-workflows).

Formatting, strict Clippy, the focused VFS regressions, Lean kernel checks, Python
type checking and prose checks run in the static job. Browser feature checks and
the playground remain separate. `tools/check.sh default` still runs the whole
native gate serially when appropriate; local resource policy takes precedence.
The post-merge deep audit retains its existing separate workflow.
New main revisions supersede older deep runs, including runs still waiting for a runner.
Each replacement keeps the complete audit inventory and the same deadlines.

The deep workflow also accepts `external_replays_only=true` for a focused,
remote check of all pinned external patch replays, split into six jobs with
15-minute deadlines. The default deep
audit still runs every check. Replay children discard inherited Rust compiler
flags, including target-specific flags, so fr's `-D warnings` policy cannot
turn new compiler deprecations in historical upstream code into task failures.
Warnings remain visible, and each upstream source tree retains its own lints.
The test launcher applies this policy before starting Python; retained historical
evaluators, results and source snapshots remain unchanged. Direct replay commands
can use `python3 tools/external-eval.py tools/agent-eval.py replay DIRECTORY`.

The runtime target excludes runner queue delays. Inspect job timestamps and shard
logs after changes to suite size; update measured weights or split work before a
job reaches its deadline. A deadline failure must not cause tests to be removed.

For example, a job queued for twenty minutes has not spent twenty minutes executing tests.
Compare `startedAt`, completed steps and the test log to find the work that used the deadline.
If one test repeatedly rebuilds an unchanged index, fix that repeated work and rerun the full test.
Keep its behavior cases and time limit intact.

In [run 37290564129](https://github.com/e6qu/fun-refactor/actions/runs/37290564129),
the single repository recipe replay exhausted its job's 15-minute deadline. Build
time was 50 seconds; the test then ran for more than 13 minutes before cancellation.
Splitting other tests cannot shorten this individual test. The repair removes an
unused test index and avoids rebuilding indexes for recipe steps that make no edits.
It preserves the complete workspace replay and tests real edits between empty steps.
The complete deep run measures the actual replay time; small fixture timings cannot establish it.

The [unmodified rerun](https://github.com/e6qu/fun-refactor/actions/runs/37297977834/job/111723664639)
passed in 777.19 seconds, with a 14m47s whole-job time. The
[repaired replay](https://github.com/e6qu/fun-refactor/actions/runs/37301872179/job/111736249724)
passed in 303.91 seconds, with a 6m52s whole-job time. Both retained the full workspace and the
same deadline. The repaired runtime's complete deep audit passed all 17 jobs. These two runner
samples establish this observed improvement, not a general speed guarantee. Queue time remains
outside these job measurements.

Active evidence references also live in Rust tests, including paths assembled with `join`.
Check those references as well as the roadmap catalog before refreshing runtime evidence.
A change to `src/cli.rs` can affect all the groups below in the
[refresh workflow](../.github/workflows/refinement-evidence.yml).

| Refresh group | What its report checks |
|---|---|
| `agent-guide-context` | Command and byte accounting for a checked scalar edit |
| `workflow-routes` | Editing routes, guided body preparation, stale-input refusals and recovery after applied-check failure; 15-minute hosted job |
| `intent-action-context` | The same reviewed edit through composed calls and an intent action |
| `completion-workflows` | Seven workflow families reach executable actions from a guide |
| `retained-proofs` | Proof records invalidate after relevant changes and replay delivery |
| `host-recovery` | Edit recovery at process-interruption boundaries |
| `index-resolution` | Complete symbol and reference answers match a pinned baseline |
| `python-repositories` | Two scripted repository edits pass behavior checks and patch replay |

Freeze the bound source inputs before dispatch, then import results from the exact successful
run after checking every source hash. Update active test references and preserve historical reports.
If workstation resource admission stops local evidence handling, dispatch
`refinement-evidence.yml` with `group=retain-python-contract` on the working branch. Supply the
successful collection's `collection_run` ID and a new dated `prefix`. The hosted job verifies
the archive digest, collection commit, producing job and attempt, current recorded input fingerprints,
CPython outcomes and tests that reject altered records before committing the report, manifest,
active test reference and generated roadmap files. It refuses failed collections and writes only
to the selected working branch. It preserves old reports and leaves acceptance obligations unchanged.
Its token-authenticated push does not start another workflow automatically; dispatch `ci.yml` on
the resulting branch revision before merging. Full CI also accepts a manual dispatch to validate
an already merged revision. Existing test and job deadlines remain unchanged.
The recovery evaluator binds `tests/host_recovery.rs` itself, so update its report path before
regeneration. This avoids generating a report that immediately becomes stale when its test changes.

The pinned Zig archive now has one checksum-verified cache shared by native jobs and the deep audit.
A five-minute toolchain check can fill that cache, but native and static jobs now queue independently.
Each already installs and checksum-verifies its own archive through `native-tools`; waiting for another
runner adds no validation. On October 5 the main-branch cache was present, yet PR #431's toolchain job
remained queued after the browser and packaging checks finished, blocking all eleven native/static jobs.
Removing that dependency preserves every check, shard and deadline. A cold cache can cause duplicate
bounded downloads; it does not permit unchecked archives or longer limits. This avoids one scheduling
barrier but cannot guarantee a 15-minute end-to-end gate when hosted runners are queued.

Every restore rechecks the archive digest. Interrupted range downloads resume verified bytes with five bounded 45-second attempts;
they retain the existing five-minute installation limit. This addresses the download timeout in
[main run 36787051550](https://github.com/e6qu/fun-refactor/actions/runs/36787051550).

The study job also exercises isolated graders and agent command containers with an image whose local
content ID it records after pulling. Offline providers drive a complete command/edit/grade attempt,
including a synthetic executable that tests the fr mount. This fixture does not measure model quality
or real fr effectiveness. Live runs must freeze their own image digest, binary and independent cases.
The same attempt reads source before and after the isolated edit and verifies that the report keeps
command-internal reads unknown. Offline cases cover UTF-8 (the text encoding used to count source bytes) paging, stale continuations, overlap across
agents, retained failures and rejection of altered disclosure counters.
The study job also prepares temporary systemd slices on its disposable Linux runner. It verifies
shared command/grader accounting, CPU-budget termination of detached descendants, and kernel memory
limits. Those tests remove their dedicated units and containers; workstation checks use fake counters
and never change local cgroups or start Docker.
The same study job runs the OpenCode rehearsal's offline parser, evidence and private-grader tests.
It does not launch OpenCode or contact model providers; live local attempts use the workstation guard.

After merging a release PR, wait for release creation before advancing main again. GitHub can reject
release creation against an older commit without an existing branch or tag, even with `contents: write`.
See its [workflow-scope rule](https://github.blog/changelog/2023-11-02-github-actions-enforcing-workflow-scope-when-creating-a-release/).
If this occurs, confirm the intended release commit and preserve it on a temporary branch using an
authorized maintainer account. Retry the failed release job, verify the tag targets that exact commit,
then delete the temporary branch. Do not retarget the release to newer code or broaden token permissions.
Run [37190700691](https://github.com/e6qu/fun-refactor/actions/runs/37190700691) recovered version 0.49.2 this way.
