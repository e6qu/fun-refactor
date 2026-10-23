# Prepare a fair comparison before paying for agent attempts

Suppose an agent must repair a string comparison that gives the wrong answer for `éa` and `éab`.
Will ordinary search and editing be enough? Does adding CodeGraph or fr help the agent finish a correct repair?
The planned comparison asks that question, then repeats it for a feature that calculates escaped text length without allocating a temporary string.

This preparation makes no model calls. It establishes neither agent adoption nor savings.
The projects are already familiar to development. A fresh agent session does not make either project an unread evaluation.

## The proposed 18 attempts

| Agent application | Unicode string repair | Escaped text length feature |
| --- | --- | --- |
| Claude Code | Ordinary tools; those tools plus CodeGraph; those tools plus fr | Same three setups |
| Codex | Same three setups | Same three setups |
| OpenCode | Same three setups | Same three setups |

Ordinary tools means reading files, searching, editing and running checks.
Each setup gets those same tools. Specialized tools must be absent from the other setups' executable search paths and application configurations.
The executable search path tells a program where to find commands such as `fr`.

Generate the proposed schedule on a GitHub runner:

```sh
python3 -B tools/agent-comparison-plan.py --prepare > comparison-plan.json
python3 -B tools/agent-comparison-plan.py --verify comparison-plan.json
```

The schedule rotates which setup goes first. Each setup occupies each position twice across the six task/application groups.
It records eighteen unstarted attempts. Missing outcomes, usage, costs and actual tool use remain unknown rather than becoming successful or free attempts.
It binds the task requirements, independent checking programs, client releases, tool packages and complete fr skill to their exact file contents.
Changing any of those inputs requires regenerating and reviewing the schedule.

The [existing task requirements](../tests/agent-eval/unknown-target/task.json) supply the same per-attempt limits:
two phases of at most twenty minutes, eighty calls and one MiB of visible output across the attempt, with one worker at a time.
A MiB is 1,048,576 bytes. The old study's eight-attempt ceiling belongs to that stopped study; this proposed eighteen-attempt comparison does not restart it.

## Check tool setup and background programs without a model

The [GitHub workflow](../.github/workflows/agent-comparison.yml) runs three separate containers.
A container gives each setup its own filesystem and processes. Each has ordinary tools and only its assigned specialized tool.
External network access is disabled while the control runs; it receives no model credentials.
The release archives are checked against recorded SHA-256 fingerprints before building the images.
Keep `/workspace` for the project and `/tmp/home` for the isolated user's home.
The first CodeGraph control put the home inside the project; CodeGraph correctly refused to index a parent of the home directory.
The corrected layout respects that boundary without forcing indexing or changing limits.

Each setup finds the same small Python function, `normalize_total`, called by `checkout`.
The fr control also uses `fr project find settings --signature` to inspect this project's new settings validator.
The CodeGraph control runs the pinned release's real `init --yes` command and `serve --mcp` server, lists its tools and calls `codegraph_explore`.
MCP is the request protocol an agent application uses to call an external tool server.
This direct protocol check complements the [existing native skill discovery checks](agent-skill.md).
The native application controls below separately test loading and using the assigned tool setup.

CodeGraph starts a background indexing program, called a daemon, that can survive the client that asked the question.
The rehearsal verifies that this happens with the pinned release: the daemon leaves the client's process group and remains alive after the client exits.
A process group is a collection of programs a host can signal together; a detached daemon creates another group.
Counting just the client's group would miss that program.

A Linux kernel resource group counts the entire container, including the scripted host, proxy, daemon and their workers.
Its explicit limits are half a CPU core, sixty CPU seconds, 512 MiB of memory, no swap and sixty-four processes.
CPU seconds count time spent executing; they differ from elapsed time while waiting.
The [pinned CodeGraph parser](https://github.com/colbymchenry/codegraph/blob/v1.6.2/src/extraction/parse-pool.ts) chooses its worker count from the host's cores, which can exceed the container's CPU allowance.
This rehearsal explicitly sets its parse and resolver worker limits to one, and sets fr's Rayon worker limit to one.
CodeGraph's query pool is explicitly disabled: these controls issue one query at a time, so its daemon handles them without an additional worker and database connection.
A worker is a program or thread doing part of the tool's work. These limits avoid competing startup work on the shared half core; they do not disable CodeGraph's daemon or increase any deadline.
The container also has bounded temporary storage and no writable host mounts.
After the query, the controller removes the container and requires an empty resource group.
It retains CPU and peak-memory counters, command output and available server diagnostics, including failures.

A separate failure control starts a busy worker, detaches it and exits its launcher.
The kernel group must catch it at half a CPU second and leave no surviving program after cleanup.
This tests the accounting mistake that a parent-only measurement would make.
The smaller budget is deliberate; no timeout is extended to make the control pass.

The artifact's elapsed time includes container creation through final shutdown.
It excludes downloading/building images, the outer controller, the Docker service and model inference.
These are integration controls, not runtime benchmarks: there is no speed ranking or language-overhead claim.
They must not be presented as complete live-attempt costs.

## Check each actual application and the task graders

The same workflow now exercises three fresh containers per application: ordinary tools, fr, and CodeGraph.
Only the assigned specialized executable is installed. The fr skill is installed only in the fr setup.
For example, Claude Code receives the same ordinary read, edit and shell tools in every setup;
the CodeGraph setup also connects its real MCP server. A scripted response asks the application to query the fixture.
The next provider request must contain the successful tool result. Errors and missing tools fail the control.

These are real pinned client binaries with empty homes and no live credentials.
Claude Code and OpenCode receive two fixed replies from a local HTTP server inside the network-disabled container.
The model name `fr-scripted-control` names that test fixture, not a model available to buy.
Codex uses its [app-server commands](https://learn.chatgpt.com/docs/app-server) to inspect effective settings,
list skills, run a declared command and call the configured MCP tool without requesting inference.
Its explicit command uses the existing container as the sandbox; this does not test an autonomous model turn.
All application, provider and background tool processes share the existing container resource limits and cleanup check.

Applications and tools can each start background threads, even for a single question.
The rehearsal limits their runtime worker pools, named Tokio, libuv and V8, to one thread each.
Node's [background worker pool](https://nodejs.org/api/cli.html#--v8-pool-sizenum) has its own setting, separate from libuv and CodeGraph's task workers.
Each application and its children are also restricted to one available CPU, under the unchanged half-core time allowance.
The time allowance alone leaves the runner's other cores visible, which can cause oversized worker pools.

Claude loads project settings so it can discover the installed skill, while its empty home excludes personal settings.
OpenCode receives a fixed session title to avoid a separate title-generation request.

A plugin adds code to an application; an fr skill supplies written instructions.
These controls use no plugins, but OpenCode would still start its package installer to prepare files for writing them.
Its [installer](https://github.com/anomalyco/opencode/blob/v1.19.3/packages/core/src/npm.ts) skips that work when configuration folders are read-only.
The control makes those folders read-only after installing the fr skill in its assigned setup.
This avoids loading an unused installer and trying to download packages inside the network-disabled container.
The control requires skill discovery to work and rejects generated package manifests or installed modules.
The schedule retains these runtime settings for every setup. They are preparation choices, not a measured performance advantage.

Native clients use `/home/control` as their isolated home, allowing Codex to create its required helper aliases outside temporary directories.
The image installs bubblewrap, Codex's Linux sandbox dependency.
Codex plugins are explicitly disabled; these controls need the installed skill and declared tools.
CodeGraph retains its daemon and liveness watchdogs in these controls. Its project path is explicit.
The Bun applications receive `BUN_JSC_useJIT=false`, disabling their runtime compiler for these setup checks.
This changes how JavaScript executes, so these controls cannot establish normal runtime performance.
The initial OpenCode and CodeGraph setup exceeded the shared 512 MiB cap; garbage-collection hints did not resolve it.
The complete regex task also reached that cap with a separate query worker; the [serial query path](https://github.com/colbymchenry/codegraph/blob/v1.6.2/src/mcp/query-pool.ts) avoids that worker and its database connection without increasing memory or time limits.
That change alone was insufficient: the daemon retained about 199 MiB while OpenCode grew beyond 320 MiB during resumption.
Limiting Node's JavaScript heap alone did not keep the resumed pair below the cap.
The Bun applications receive `BUN_OPTIONS=--smol`, its [documented low-memory runtime mode for standalone executables](https://bun.com/docs/bundler/executables#runtime-arguments-via-bun-options).
This trades execution speed for more frequent collection of unused memory. Native allocations, temporary filesystem contents and every watchdog still count toward the unchanged shared cap.
Diagnostics distinguish anonymous memory from filesystem-backed memory, and the workflow runs the larger OpenCode tasks first.
Agree the actual live-attempt runtime settings and include their costs before comparing agent performance.

The generated schedule includes exact rehearsal configuration objects and launch arguments for all nine setups.
It also includes discovery and delivery prompts for both tasks. Every application and setup gets the same task wording.
The discovery prompt asks for a `HANDOFF.md` diagnosis; the delivery prompt asks the next process to read it,
inspect current source, implement the repair and export `change.patch`.
The small fixture controls check tool connections. The complete-project controls below execute both phases with these prompts.
The rehearsal configurations deliberately use a scripted provider and a single controlled CodeGraph query.
Agree and freeze the complete offered tool set and real model settings before live attempts.

For each task, a separate GitHub job runs the tests shipped with the pinned project and the existing independent behavior checker:

- Unchanged Unicode code must fail the 67,081-pair behavior check. The original regex code must fail because the new public API is absent.
- An incomplete Unicode repair fixes one length calculation but leaves the other wrong. The incomplete regex change adds length APIs but leaves extra allocations. Both must be rejected.
- The reference repair must pass upstream and independent checks. Its Git patch must also pass in a fresh receiver. Undo must restore the original tracked source exactly; reapplying must restore the repaired source exactly.

Run these controls on GitHub with `python3 -B tools/agent-comparison-grade.py controls --task TASK --output NEW_DIRECTORY`.
The schedule records the separate `grade --candidate CANDIDATE_DIRECTORY` command for later submissions.
Keep grading outside the candidate directory. The current controls execute trusted pinned fixtures on a dedicated runner;
they do not establish isolation for arbitrary hostile submitted code.
Failures retain their diagnostics. A timeout or compilation error cannot become the expected missing-API result.
Private checkers stay outside the application container. Trusted task-control images contain prescribed repair recipes, as explained below.

## Resume a complete task after another person edits the code

Suppose an agent is repairing Unicode text comparison and saves its diagnosis before stopping.
While it is paused, another maintainer renames the matching-pair counter from `intersection_size` to `shared_bigrams`.
The old reviewed edit refers to the previous source. Resuming must reject it and prepare a new repair that keeps the maintainer's rename.

The workflow rehearses that sequence for both complete pinned Rust projects in all nine application/tool setups: eighteen controls.
For the escaping task, the intervention renames the output variable from `quoted` to `escaped`.
Escaping turns punctuation such as `.` into `\.` so a regular expression treats it as plain text.
The repair must still provide the requested byte-length APIs and reserve enough space for the escaped output.

1. Start the application with the discovery prompt. Inspect the assigned code and save `HANDOFF.md` plus a reviewed, unapplied repair.
2. Let that application process exit. Rename the local variable and record the new source as the delivery starting point.
3. Start a fresh application process with the delivery prompt. Verify the handoff, refuse the old edit without changing source, and review the current implementation.
4. Apply the refreshed repair, undo it exactly to the intervening source, then redo it exactly. Export only the repair as `change.patch`.
5. Remove the application container and require complete process cleanup. Apply the patch in a separate checkout containing the same intervening rename.

Before pausing, the old review must pass a preview or patch check without applying it.
Control logs stay outside the project, so writing diagnostics cannot make that review stale.

The fr setup uses a handle, an identifier for a selected function or file in the inspected source.
Its saved plan keeps the reviewed edits and their starting contents.
The control applies, undoes and redoes that plan through fr history.
It batches the declaration and file lookups into one fr query, sharing one inspected project view instead of rebuilding it for each target.
The prescribed fr edits explicitly limit analyzed source files to 65,536 bytes. Both task files and their repaired forms must fit that limit.
The regex workspace includes larger generated Unicode tables and parser files; parsing all of them for a selected-function edit exhausted Codex's ten-second command window.
Those larger files stay in the complete checkout and in the independent tests, but are outside this control's fr analysis coverage.
This does not establish complete caller discovery or whole-workspace analysis within ten seconds.
The ordinary-tools and CodeGraph setups use reviewed Git patches for editing and reversal.
CodeGraph must answer the assigned function query through the application's actual tool connection in both phases.
The check does not establish that every CodeGraph finding is current after arbitrary source changes.

The application runs `git diff --check`, which checks patch whitespace, and reports upstream tests as pending.
After its container is removed, the independent grader runs the project's existing Rust tests and behavior checks on the receiving checkout.
It checks that the local-variable rename remains present, rejects unexpected source paths, and reverses and reapplies the delivered patch exactly.
Compilation errors and timeouts cannot establish a passing repair. Failures retain command output and resource diagnostics.

These are trusted scripted controls. The repair recipe supplies known operations; no model discovers or chooses a solution.
Claude Code and OpenCode consume fixed provider replies; Codex uses its native command and tool APIs without starting inference.
Task-control images include those prescribed recipes and must not be used for live agents.
Private graders are not copied into those images. The proposed eighteen live attempts remain unstarted, with outcomes, usage and costs unknown.

The application processes and background tools retain the existing shared 512 MiB, sixty CPU seconds and sixty-four-process limits.
Both phases share one container budget. Each application invocation and tool call retains its existing deadline.
The private Rust grading runs afterward on the GitHub runner, outside those container counters.
Image preparation and grading are not included in a complete cost result here; these controls make no performance or savings claim.
Full-attempt grading isolation, credential access and resource admission still need a separate live-trial review.

To run one prescribed control on GitHub:

```sh
python3 -B tools/agent-comparison-handoff.py \
  --host claude --task unicode-dice --setup fr --output NEW_DIRECTORY
```

`--host` names the application, `--task` selects one of the two pinned tasks, and `--setup` selects its offered tools.
Use a new output directory so earlier failures remain available. Full builds and these controls run on GitHub, not a shared workstation.

## What must happen before live attempts

Choose and agree exact models, reasoning settings, prices and spending caps.
Within each application, all three setups use that application's same model settings.
The preparation command can review an explicit settings file:

```sh
python3 -B tools/agent-comparison-plan.py --review-settings agreed-settings.json
```

It requires all three applications, their provider/model identities, reasoning settings and all four token prices: ordinary input, cached input reads, cache writes and output.
Each price needs its source and date. It requires positive total and per-attempt spending caps; missing prices do not become zero.
Reviewing the file does not authorize spending or enable live calls.
This checks required fields and numeric prices. It does not resolve model aliases or validate each provider's reasoning options; those require the native application rehearsal.
The [older paired Codex runner](agent-codex-runner.md) now also rejects undeclared model settings and requires explicit model, effort, service tier, executable and timeout.
Its `freeze-settings` command declares the agreed settings in both fresh sessions before a run.
This belongs to the Codex runner: the shared evaluator and its recorded comparison fingerprints remain unchanged.
This fixes implicit settings in that runner; it does not turn its two-setup protocol into this proposed three-setup study.

Then freeze the live application configurations and offered tools, fr source revision and binary, credential access and full-attempt resource limits.
Carry the complete-task rehearsal into the agreed live configurations, including independent private grading under those limits.
For Codex, verify the chosen release's behavior against its [configuration reference](https://developers.openai.com/codex/config-reference) and [app-server interface](https://developers.openai.com/codex/app-server).
The container control alone does not establish native application integration, correct grading or bounded live model usage.

Count instructions, installation, indexing, model use, retries, edits, checks and shutdown in live results.
Report first use with setup included; report later use separately.
Grade behavior and replay the patch in a fresh checkout with the same independent checks for every setup.
Keep each first failure, timeout and offered-but-unused tool visible.
The earlier stopped studies stay stopped. The [product review](product-review.md#first-comparison-with-codegraph) owns the comparison's wider limits.
