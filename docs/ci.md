# Run the complete checks without slowing the workstation

When you open a pull request, GitHub divides the native and Python tests across runners and checks that none are missing.
If a job times out, find what made it slow or stuck. Keep the existing deadline while fixing that cause.

CI means continuous integration: GitHub runs checks for a proposed change.
A workflow is a declared set of jobs. A shard is one assigned portion of the test suite.
The final gate checks that all required portions ran and passed; a passing small portion is insufficient.
Queue time is time waiting for a runner. Execution time starts when a runner begins work.
For a slow PR, inspect those times separately before deciding which operation needs repair.

## Keep completed project checks reproducible

CI replays the [recorded project changes](project-change-milestone.md) and checks that altered completion claims fail.
The record covers saved answers after seven changes on three projects, plus complete Rust and Python renames.
For example, Boltons' renamed integer-bounds function must preserve its outputs, documentation references, undo and patch delivery.

The replay uses the recorded source, Rust 1.99.1 and Python 3.14.8.
It preserves the meaning of Humanize's first evaluation; a replay makes no new unread-project claim.
Another required job checks the current replay with both current checker builds.
Both builds use Rust 1.99.1, so a compiler update cannot silently change the program being checked.
It repeats all 21 change checks and both deliveries; all three projects are familiar during this maintenance run.
The ordinary native and Python suites also check the current code.
The final required gate includes these records and the earlier original-package checks.

The migration test also appears in the project-wide test inventory.
Publishing a replacement result updates that entry after its behavior and patch checks pass.
The inventory then checks the same test file that CI runs.

The development collector also requires a clean checkout: its files must match the commit named in its result.
If formatting or preparation changes files, the job fails and saves those corrections.
Commit the corrections before qualification. Successful source reads alone do not qualify the experiment.

An original source snapshot can contain its own `.gitignore`, a list of filenames Git should omit.
JDcal's list names `MANIFEST`, even though its recorded source archive includes that file.
The `project-record-files.yml` packaging workflow authenticates the original collection and checks every file after Git's storage and checkout conversions.
The checked-out bytes must match the original collection, including line endings. Git must also report no uncommitted file changes after staging.
This also preserves bytes when a snapshot's own text-conversion rules would otherwise change them.
It checks each staged file against the original bytes before committing. Ordinary validation continues to fail when a declared file is missing.
This packaging step preserves the fixed evaluation inputs and their original fingerprints.
It also regenerates and checks the tracking pages before publishing the repaired record.

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

## Collect Python results while checking the interface

The `python-contract` collection job checks two independent questions.
Does the Python interface return the expected traces? Do the saved analyzer answers agree with the declared Python examples?
For example, the interface checks `choose(read_input())` after a stored list supplies the helper's default.
The collector separately runs the declared programs and saves their Python and analyzer results.

Those tasks read the compiled executable and use separate temporary projects.
The [collection runner](../tools/python-contract-runner.py) starts the interface checks while collecting reports.
It waits for both tasks before checking that altered records fail validation.
If either task fails, the job fails and keeps its diagnostics.
The [runner tests](../tools/test-python-contract-runner.py) check that both failures remain visible and that an audit cannot run before both tasks finish.

The interface and collection processes each have a 600-second limit.
The altered-record audit has 120 seconds. The complete job keeps its fifteen-minute deadline, including setup, upload and cleanup.
The interface log is saved separately as `python-contract-sdk-checks`; the result artifact remains `python-contract`.
A cancelled job cannot qualify as a successful collection, even when its report and individual tests passed.

The `scalar-development` option runs focused development checks in two hosted jobs.
One compiles and tests the analyzer, then checks Python arguments and [code run during import](check-python-startup.md).
The other checks imports and saved answers. Both keep a fifteen-minute deadline.
The [development history](evaluations.md#check-a-small-function-that-fills-a-table-during-import) records why running every group after compilation exceeded that deadline.

This changes the order of CI work. Analysis speed and complete agent costs need separate measurements.

## Find repeated work in Python report checks

An audit compares saved answers with the declared source files, checking rules and Python observations.
If its controls time out, measure their work before changing anything.
Use a successful `python-contract` collection as the fixed input:

```sh
gh workflow run refinement-evidence.yml --ref YOUR_BRANCH \
  -f group=python-report-audit -f prefix=YOUR_NEW_NAME \
  -f collection_run=SUCCESSFUL_PYTHON_RUN_ID
```

Replace `SUCCESSFUL_PYTHON_RUN_ID` with the number in that collection's GitHub Actions URL.
The diagnostic downloads its original report, runs every audit control, and saves call counts and elapsed times as `python-audit-profile`.
It keeps the existing 120-second audit deadline and stops with a stack trace if that limit is reached.
This measures the audit of that report; it does not measure analysis speed or overhead compared with Python execution.

Keeping a capture also requires checking which code produced it.
A documentation edit creates a different commit while leaving the analyzer unchanged.
Retention compares the analyzer's source folders, local dependencies, parsing queries, Python client, build script and dependency manifests with the current checkout.
Each capture keeps its actual collection commit. Every recorded input still receives its existing report audit.
The retained Python report uses compact JSON to avoid storing indentation across hundreds of examples.
Its `artifact.zip` keeps the exact downloaded archive, including the original report bytes.
The reader verifies both files, the original report's fingerprint and the compact copy's exact agreement with the captured values.
A missing archive, changed value or obsolete receipt stops the check; the current reader accepts one receipt format.
This changes storage only. The source identity, cases and recorded measurements keep their original values.
Changed analyzer code, changed recorded inputs and uncommitted analyzer files stop retention.
This lets unrelated captures remain useful after a CI-only edit without changing their contents or fingerprints.

Native tests select the current collection through the compiler report listed in the project tracker.
The checksum is a fingerprint of a file's exact contents.
They require its collection receipt and check each requested report's checksum before running the existing audit.
For example, saving new call examples changes the current collection once; tests no longer keep separate dated links to older reports.
A missing receipt, changed report or disagreement with the tracker fails the check.

## How the checks run

PR checks have a 15-minute job deadline. Native tests run on six GitHub runners;
the Python SDK runs on four more. Each runner executes tests serially with one
Lean worker. Local builds and resource limits do not change.

Python test dependencies come from an exact list of package files and SHA-256 fingerprints.
For example, a test job needs FastAPI and the packages that FastAPI imports.
Previously, eleven jobs each asked the package index which files to use.
A [recorded failure](https://github.com/e6qu/fun-refactor/actions/runs/39214145764/job/117933990993) stopped during a FastAPI metadata download after the connection reset.
Metadata describes a package before installation; no test had started in that job.

The [current file list](../.github/actions/native-tools/python-wheels.txt) pins all nineteen packages from a passing job, including their dependencies.
Jobs share a cache of those exact wheels, which are installable Python package archives.
A cold cache downloads the pinned files directly, without package-index searches or separate metadata requests.
Downloads have a two-minute bound. Every install reads local files, checks their fingerprints and verifies dependency consistency.
Missing or damaged files fail visibly. Installation never switches to an online package search.
Jobs still queue independently and retain the complete test inventories and existing 15-minute deadlines.
The commands use pip's [hashed requirements](https://pip.pypa.io/en/stable/topics/secure-installs/) and [offline installation](https://pip.pypa.io/en/stable/topics/repeatable-installs/).

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
| `checked-outcomes` | A wrong repair fails its behavior check; a correct repair can be reopened and delivered |
| `model-comparisons` | Rust and Python answers agree on eight true-or-false input combinations, and saved model proofs can be reopened |
| `resumable-correspondence` | A saved function selection is checked after moves, renames, copies and deletion |
| `host-recovery` | Edit recovery at process-interruption boundaries |
| `index-resolution` | Complete symbol and reference answers match a pinned baseline |
| `python-repositories` | Two scripted repository edits pass behavior checks and patch replay |

Freeze the bound source inputs before dispatch, then import results from the exact successful
run after checking every source hash. Update active test references and preserve historical reports.

Current regression tests need current records. For example, after changing answer storage to SQLite,
an old saved delivery receipt cannot exercise reopening with the new store. Collect `checked-outcomes`,
`model-comparisons`, `retained-proofs` and `resumable-correspondence` from one source commit.
Then dispatch `retain-ci-reports` with a new `prefix` and an `evidence_runs` JSON map from those
four group names to their successful run IDs. The publisher checks complete archives, current code
and behavior before updating the test paths and tracking. Model replay also runs `fr` and Lean,
the proof checker. Earlier records remain unchanged; the current SDK gains no older-format reader.

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

UTF-8 encodes text as bytes. For example, `é` occupies two bytes, so byte limits can differ from character counts.

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

Fresh grading of saved code changes runs separately in `Code-change grader controls`.
That job checks the same controls and saved submissions, writes the comparison report and uploads `native-code-change-grades`.
Accounting and grading each keep a five-minute deadline; the final native/SDK gate requires both.
The [earlier combined job](https://github.com/e6qu/fun-refactor/actions/runs/39268191916) passed its test steps but exceeded five minutes during final cleanup.
Separating those independent workloads keeps every check and leaves room for checkout, upload and cleanup.

After merging a release PR, wait for release creation before advancing main again. GitHub can reject
release creation against an older commit without an existing branch or tag, even with `contents: write`.
See its [workflow-scope rule](https://github.blog/changelog/2023-11-02-github-actions-enforcing-workflow-scope-when-creating-a-release/).
If this occurs, confirm the intended release commit and preserve it on a temporary branch using an
authorized maintainer account. Retry the failed release job, verify the tag targets that exact commit,
then delete the temporary branch. Do not retarget the release to newer code or broaden token permissions.
Run [37190700691](https://github.com/e6qu/fun-refactor/actions/runs/37190700691) recovered version 0.49.2 this way.

## Keep saved answers tied to the checking code

If we fix how `fr` reads Python, an answer saved by the earlier checker needs checking again.
The same applies to saved information about functions and imports.
[`native-source-inputs.txt`](../native-source-inputs.txt) lists the checking program's source roots and the generated directories to ignore.
A compiler turns the Rust source into the program we run.
The build hashes every declared source file, including generated grammar tables, with the compiler version and build settings.
A missing or unreadable input stops the build.
A symbolic link points to another file or folder. These links are refused inside the declared source roots.
The result collectors use that same list, so changing a grammar or a helper file cannot escape their source checks.
Release labels retain the existing normalization used by these recorded behavior comparisons; dependency and code changes remain bound.

The focused runner changes source, grammar, query and dependency inputs in temporary projects.
It also checks file additions, deletions, renames, build-setting changes and ignored build outputs.
The full native gate runs these controls alongside the existing cache tests.
When these inputs change, `fr` computes fresh answers using the current checking code.


## Recheck completed timing studies with their original code

A language change makes an earlier timing report historical.
It cannot establish the new checker's cost, and changing its recorded source fingerprints would hide that difference.
The study job checks out commit `8ca25c2d72484b7e66f49b279f8b5009c68f3d6e` separately for the three completed Python cost comparisons.
It requires a clean checkout at that exact commit before and after running the original checks.
It also compares the six published reports byte for byte with the original committed records.
The current checker keeps its own tests; historical audits use the original SDK and checking scripts.
This introduces no alternate format reader in the current SDK.

The first [source-preservation attempt](https://github.com/e6qu/fun-refactor/actions/runs/39244309260) stopped at the archive reader's existing 64 MiB expansion limit.
One generated grammar file alone expands to about 83 MiB.
A separate Git checkout handles these original tracked inputs without increasing the archive limit.
No measurement or recorded fingerprint changed.
