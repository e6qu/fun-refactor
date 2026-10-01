# Rehearse small tasks through OpenCode

`tools/opencode-rehearsal.py` compares ordinary source operations with the same operations plus
bounded `fr` CLI discovery. It uses an existing local OpenCode installation and its configured
provider authentication. It never copies credentials into the repository, changes global config,
or installs OpenCode. The initial profiles name `kimi-code-plan-global/k3` and
`zai-coding-plan/glm-5.3-flash`; these are the configured IDs observed on October 1, 2026.

This is a small local rehearsal, separate from the [full independent study](agent-study.md).
The supplied tasks are synthetic. Their graders check finite behavior and reject the original
broken implementations; the agents receive requirements without edit locations or expected patches.
Neither these tasks nor the constrained tool protocol establish unfamiliar-repository performance,
native OpenCode workflow quality, general token savings, or formal proofs.

## Freeze the comparison

Use the workstation resource guard for every command below. The examples spell out its path;
other machines must provide equivalent limits before running live attempts.

```sh
python3 /Users/zardoz/.codex/tools/fr-local-guard.py \
  python3 tools/opencode-rehearsal.py freeze tests/agent-eval/opencode/manifest.json \
  --binary target/debug/fr > /tmp/opencode-plan.json
```

The plan embeds bounded source bundles and binds grader bytes, runner implementation, instructions,
binary identity, model IDs, repetitions and randomized arm order. It contains eight cells: two tasks,
two models and two arms, with one attempt each. Run cells in their frozen order. Choose requirements
and independent graders before changing the tool to suit a task. The included synthetic tasks are
protocol controls and are unsuitable as a held-out project.

Custom manifests use the same schema. Source directories must contain at most 100 regular files
and 1 MiB of contents; symlinks refuse. Each grader is a trusted, reviewed Python program outside
the source directory. It receives the submission directory as its first argument and a JSON object
containing the final answer on stdin. It prints `passed`, `checks` and `scope` as a JSON object.
The bundled graders restrict submitted syntax and builtins before evaluating their small functions.
They cannot run imports or general application code. Unsupported syntax fails these fixture grades.
Review any replacement grader before running it locally; the process monitor is not an OS sandbox.

## Run one cell

```sh
python3 /Users/zardoz/.codex/tools/fr-local-guard.py \
  python3 tools/opencode-rehearsal.py run /tmp/opencode-plan.json CELL_ID \
  target/opencode-attempts --base tests/agent-eval/opencode \
  --binary target/debug/fr --opencode /path/to/opencode --confirm-agent-spend
```

The spend flag acknowledges provider traffic. OpenCode does not expose the reservation interface
used by the study gateway, so this route cannot enforce a dollar cap. Coding-plan usage can have
separate subscription or account limits. A reported zero cost is retained as a CLI value; actual
dollar cost remains unknown. Do not use this route where a strict dollar cap is required.

Each attempt has at most eight model responses and 120 seconds. Individual subprocesses have a
45-second deadline, 768 MiB sampled process-group RSS, 20 sampled CPU seconds, a 1 MiB output cap
and a 16 MiB workspace-growth cap. Retained attempt data also has a 16 MiB limit. The outer guard
additionally serializes work, throttles CPU toward half a core, keeps aggregate RSS below its 1 GiB
sampled limit, preserves 64 GiB free disk and caps target data at 2 GiB. Stops remain failures;
limits are never raised automatically. Full builds, large task runs and full gates belong on runners.

OpenCode runs in a temporary directory outside the repository, with native tools denied, plugins
disabled, sharing disabled, snapshots disabled, automatic compaction disabled and no LSP downloads.
Provider configuration remains local. This avoids inheriting the repository's parent instructions;
it does not prove the absence of all global OpenCode instructions or provider-side behavior.
The host responds to one JSON action at a time using the same OpenCode session. A single complete
JSON action surrounded by prose is accepted and marked `embedded_json`; multiple proposals refuse.
OpenCode's step limit is two because a limit of one injects a forced-summary instruction.

Both arms can list files, search literal text, request source pages and replace unique text.
Only the `fr` arm can request public `project map`, `find` and `show` commands with fixed bounds.
These invoke the existing binary; no alternate analyzer is implemented in the evaluator.
There is no shell action, arbitrary command execution or child-agent action. The current route
therefore measures a constrained discovery comparison, not the full public skill or edit lifecycle.
The host grades a fresh copy containing only submitted source, excluding `fr` caches and private cases.

## Inspect every result

```sh
python3 /Users/zardoz/.codex/tools/fr-local-guard.py \
  python3 tools/opencode-rehearsal.py report /tmp/opencode-plan.json target/opencode-attempts
```

Every cell stays visible, including pending, interrupted, failed and incorrectly solved attempts.
An existing attempt directory refuses a rerun. An implementation change requires a new frozen plan;
retain the old attempt and describe the protocol change instead of replacing its failure.

Each attempt retains raw CLI streams, a final OpenCode session export when available, requested actions,
delivered results, submission, private grade and subprocess measurements. The auditor verifies artifact
hashes, re-parses successful turns, compares exported model IDs and usage, and checks grade and submission
identities. These are trusted-host records, not independently authenticated provider receipts.

Input, output, reasoning, cache read/write and total token fields remain OpenCode's reported counters.
They are not silently converted to the provider gateway's accounting schema. Actual billing, provider
cache control, global OpenCode storage growth, escaped processes, full-system resource use and complete
context accounting remain unknown. `audit_complete` and `provider_usage_verified` remain false.

The offline CI suite exercises parsing, refusals, retained failures, bounded subprocesses and grader
controls. CI never invokes these configured models or requires developer credentials.

OpenCode interfaces: [CLI](https://opencode.ai/docs/cli/),
[permissions](https://opencode.ai/docs/permissions/), [configuration](https://opencode.ai/docs/config/).
