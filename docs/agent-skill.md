# Give an agent a short guide to fr

Install this skill when you want an agent to know how to find code, preview a change, run checks and undo a failed attempt.
For example, an agent asked to fix checkout should load the relevant guide when it needs it, rather than read every command first.

A skill is an instruction bundle an agent can read. It contains a short entry page and task-specific references.
Installing the bundle makes instructions available; it does not run commands or grant write permission by itself.
For a bug fix, the agent loads the editing reference when it reaches the edit, then the check reference before testing.
Skill text counts toward what the agent reads; comparisons must count that text too.

## Commands, results and limits

The portable [`fr` skill](../skills/fr/SKILL.md) routes an agent to small references for exploration,
authoring, checks, history, Git and Lean. Native release archives include the complete `skills/fr/`
directory beside the binary.

## Install the handoff

This repository already exposes the shared bundle through [.claude/skills/fr](../.claude/skills/fr)
and [.agents/skills/fr](../.agents/skills/fr). Claude Code reads the first; Codex reads the second; OpenCode discovers both.
The entries are relative links to `skills/fr`, so instructions and examples have one maintained source.
Keep the links and their target when checking out the project. `fr` must still be on the agent's `PATH`.

Use the steps below to install the skill for other projects.
Put the matching `fr` binary on the agent's `PATH`. Run the copy commands from the source checkout
or unpacked release directory containing `skills/fr`. Choose the block for your agent.
Copy the whole directory so its internal references remain available.

For [Claude Code](https://code.claude.com/docs/en/skills):

```sh
mkdir -p "$HOME/.claude/skills/fr"
cp -R skills/fr/. "$HOME/.claude/skills/fr/"
```

For [Codex](https://learn.chatgpt.com/docs/build-skills):

```sh
mkdir -p "$HOME/.agents/skills/fr"
cp -R skills/fr/. "$HOME/.agents/skills/fr/"
```

For [OpenCode](https://docs.opencode.ai/docs/skills/):

```sh
mkdir -p "$HOME/.config/opencode/skills/fr"
cp -R skills/fr/. "$HOME/.config/opencode/skills/fr/"
```

OpenCode also discovers the Claude Code and Codex locations above. Install one copy visible to OpenCode to avoid duplicate names.
For a project-only installation, use `.claude/skills/fr/`, `.agents/skills/fr/` or `.opencode/skills/fr/`, respectively.
The source directory `skills/fr/` alone is not a documented discovery location for these agents.

Open a new local agent session and check that `fr` appears among its available skills.
In Claude Code, invoke `/fr`; in Codex, select it with `$fr` or `/skills`.
In OpenCode, ask the agent to load the `fr` skill with its skill tool.
Then ask it to find a known function using `fr project find NAME --signature` and explain the result.
Check the recorded tool call: listing a skill does not establish that the agent used `fr`.
Our [native discovery checks](https://github.com/e6qu/fun-refactor/actions/runs/39746584042) pass for all three applications on GitHub's Linux runners.
They check an absent skill, project installation, user installation and the repository's linked bundle in isolated settings.
Claude Code uses a local scripted response; no paid model or live credentials take part.
Discovery establishes that instructions are available. It does not establish that an agent chooses or follows them during an unfamiliar task.

Start with [your first checked change](../skills/fr/references/first-change.md).
It creates a small greeting program and test, finds the function and caller, previews a rename, runs the test and undoes the change.
The [introductory lesson check](https://github.com/e6qu/fun-refactor/actions/runs/39746584042) executes those commands against the pinned released `fr`.

Ordinary inspection and source history need only `fr`. Git actions need Git, and strict Lean verification needs the target project's Lean and Lake toolchain.

## Use the bounded change loop

1. Run `fr --json audit` to obtain current support, workflow and trust summaries.
2. Express the task as an `fr-agent-goal-1` document or start with a focused `project` query.
3. Run `fr --json guide --from goal.json` and follow the returned actions in order.
4. Supply only requested scalar values, fragments, semantic IR, migration choices or proof tactics.
5. Reveal source through a bounded query only when structured evidence cannot answer the question.
6. Review the complete preview, revision and basis before any write.
7. Save and apply the unchanged plan, run declared checks, then export a patch.
8. Retain the transaction ID for exact undo, redo and recovery.

Known declarations can go directly to `project find`. Use a shallow `project map` when the hierarchy
is unknown. A `project batch` can combine bounded lookup, relationship, call, test and gap queries
against one revision.

Targeted declaration rows and exact guide, disclosure and intent targets bind `location.name` and
`location.definition` to both exact source byte ranges and 1-based, half-open line and column ranges.
Request the `location` field explicitly on broad maps. Use the source byte range for source edits and the
range for display or editor navigation; both belong to the report's project revision.

```sh
fr --json project find render --signature
fr --json project show '<HANDLE>' --relations --limit 8
fr --json project disclose '<HANDLE>' --view evidence --depth 3 --token-limit 4096
```

Use built-in refactors for supported mechanical changes. Use `author` for declaration bodies,
signatures and coordinated batches. Use semantic bodies, deltas or intents when the agent can avoid
source text. The [workflow guide](agent-workflow-guide.md) gives the exact goal and action schemas.

## Review and execute

A mutation previews a diff unless the command includes `--write`. Save an accepted preview and use
its immutable transaction rather than reconstructing the command.

```sh
fr rename old_name new_name --save-plan
fr history show '<TX>'
fr history apply '<TX>' --write --no-diff
fr checks
fr checks --run '<CHECK_NAME>' --basis '<CHECK_BASIS>'
fr history patch '<TX>' --output change.patch
```

Replace the check name and basis with values from the listing. Choose a check that reproduces
the reported bug. `fr checks` by itself lists commands; it does not run them.
If the selected check fails, inspect the applied transaction and correct or undo it before delivery.

The request to inspect or export a patch does not grant permission to commit or publish. Keep source
history, Git staging and worktree journals as separate review boundaries.

## Use the Python runtime

The optional zero-dependency package mirrors the wire IR with typed Python values. It retains
intermediate reports locally, verifies identities and can store Merkle objects outside the project.

```sh
python3 -m venv .venv
.venv/bin/python -m pip install ./sdk/python
```

Read the [runtime guide](agent-runtime-sdk.md) for constructors and local guide completion. The
[context workspace](agent-context-workspace.md) explains transcript boundaries.

## Validate the skill

The checker executes every shell example in the skill against temporary projects. It substitutes
real handles and transaction IDs, checks stale-basis refusals, compiles authored Rust, builds the
Lean fixture and verifies patch application in a separate Git receiver.

```sh
python3 tools/check-agent-skill.py --fr target/debug/fr
cargo test --test agent_skill
```

The test also enforces size limits for the entrypoint, references and task routes. Historical agent
results and context measurements live in the [evaluation evidence guide](evaluations.md).
