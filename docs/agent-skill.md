# Hand `fr` to an agent

The portable [`fr` skill](../skills/fr/SKILL.md) routes an agent to small references for exploration,
authoring, checks, history, Git and Lean. Native release archives include the complete `skills/fr/`
directory beside the binary.

## Install the handoff

Put the matching `fr` binary on the agent's `PATH`. Copy the whole skill directory so its internal
references remain available. For Codex:

```sh
mkdir -p "$HOME/.codex/skills"
cp -R skills/fr "$HOME/.codex/skills/fr"
fr --version
```

For another agent, point its instruction loader at `skills/fr/SKILL.md`. Ordinary inspection and
source history need only `fr`. Git actions need Git, and strict Lean verification needs the target
project's Lean and Lake toolchain.

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
`location.definition` to both exact byte spans and 1-based, half-open line and column ranges.
Request the `location` field explicitly on broad maps. Use the byte span for source edits and the
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
fr history patch '<TX>' --output change.patch
```

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
