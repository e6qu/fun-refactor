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
This rehearsal explicitly sets its parse, query and resolver worker limits to one, and sets fr's Rayon worker limit to one.
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

The generated schedule includes exact rehearsal configuration objects and launch arguments for all nine setups.
It also includes discovery and delivery prompts for both tasks. Every application and setup gets the same task wording.
The discovery prompt asks for a `HANDOFF.md` diagnosis; the delivery prompt asks the next process to read it,
inspect current source, implement the repair and export `change.patch`.
These prompts describe the planned handoff. The native fixture controls do not execute that complete two-phase task.
The rehearsal configurations deliberately use a scripted provider and a single controlled CodeGraph query.
Agree and freeze the complete offered tool set and real model settings before live attempts.

For each task, a separate GitHub job runs the pinned upstream tests and the existing independent behavior checker:

- Unchanged Unicode code must fail the 67,081-pair behavior check. The original regex code must fail because the new public API is absent.
- An incomplete Unicode repair fixes one length calculation but leaves the other wrong. The incomplete regex change adds length APIs but leaves extra allocations. Both must be rejected.
- The reference repair must pass upstream and independent checks. Its Git patch must also pass in a fresh receiver, reverse to the original tracked source exactly and reapply exactly.

Run these controls on GitHub with `python3 -B tools/agent-comparison-grade.py controls --task TASK --output NEW_DIRECTORY`.
The schedule records the separate `grade --candidate CANDIDATE_DIRECTORY` command for later submissions.
Keep grading outside the candidate directory. The current controls execute trusted pinned fixtures on a dedicated runner;
they do not establish isolation for arbitrary hostile submitted code.
Failures retain their diagnostics. A timeout or compilation error cannot become the expected missing-API result.
Reference implementations and private checker details stay outside agent containers and prompts.

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
Rehearse both complete task phases, their intervening source change and independent private grading under those limits.
For Codex, verify the chosen release's behavior against its [configuration reference](https://developers.openai.com/codex/config-reference) and [app-server interface](https://developers.openai.com/codex/app-server).
The container control alone does not establish native application integration, correct grading or bounded live model usage.

Count instructions, installation, indexing, model use, retries, edits, checks and shutdown in live results.
Report first use with setup included; report later use separately.
Grade behavior and replay the patch in a fresh checkout with the same independent checks for every setup.
Keep each first failure, timeout and offered-but-unused tool visible.
The earlier stopped studies stay stopped. The [product review](product-review.md#first-comparison-with-codegraph) owns the comparison's wider limits.
