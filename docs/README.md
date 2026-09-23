# Documentation

Start with the [project README](../README.md) to install `fr` and run a first command. Use this page
to choose one authoritative guide for the task; detailed contracts link to related material instead
of repeating it.

## Use `fr`

- [Tutorial](../TUTORIAL.md): complete a first inspected, reviewed and reversible change.
- [CLI reference](../CLI.md): every command, option and output contract.
- [Examples](../EXAMPLES.md): short task-oriented command sequences.
- [Recipes](../RECIPES.md): declarative multi-step refactoring.
- [Cross-language behavior](../CROSS_LANGUAGE.md): language resolution and edit rules.
- [IR reference](../IR.md): syntax and semantic intermediate representations.
- [HTTP contracts](../API_CONTRACTS.md): declared service-contract analysis.
- [Terminology](terminology.md): identities, bases, evidence and lifecycle terms.

## Integrate an agent

Begin with the quickstart, then choose the runtime or protocol detail needed by the integration.

- [Agent quickstart](agent-skill.md): install and use the portable skill.
- [Goal and workflow contract](agent-workflow-guide.md): route structured intent to reviewed work.
- [Python runtime](agent-runtime-sdk.md): keep intermediate reports outside the model transcript.
- [Context workspace](agent-context-workspace.md): materialize and cache selected Merkle subtrees.
- [Context protocol](agent-context-protocol.md): packet, continuation and review identities.
- [Progressive disclosure](progressive-disclosure.md): reveal semantic, project and source branches.
- [Completion audit](completion-audit.md): inspect live support and trust boundaries.
- [Evaluation evidence](evaluations.md): reproduce and interpret deterministic and live-agent runs.
- [Codex runner](agent-codex-runner.md): run optional paid live-agent evaluations.

For a change, the normal path is
`audit → goal → guide → bounded evidence → preview → reviewed execution → checks → patch`.
The [goal guide](agent-workflow-guide.md) owns this contract; the other pages explain individual
stages.

## Understand and change code

- [Semantic model](semantic-model.md): source-free bodies, changes and scalar intents.
- [Body authoring](body-authoring.md): replace bodies and coordinate declarations.
- [Disclosed editing](disclosed-editing.md): edit through returned scalar and typed-IR capabilities.
- [Cross-stack surfaces](cross-stack-surfaces.md): CSS, Tailwind, Markdown and Mermaid facts and edits.
- [Framework semantics](framework-semantics.md): React, Next.js, Express and FastAPI recognition.
- [Application IR](application-ir.md): framework-neutral routes and components.
- [Feature migration](feature-migration.md): reviewed migration and runtime evidence.

## Verify and recover

- [Project checks](project-checks.md): declare compiler, test and lint commands.
- [Lean support](lean-specs.md): proof evidence, kernels and strict correspondence checks.
- [Agent formalization](agent-formalization.md): generate a project-specific proof workbench.
- [File transactions](file-transactions.md): atomic file plans, application and recovery.
- [Browser history](browser-history.md): bounded history in the WASM interface.

## Use Git

- Inspect [status](git-status.md), [changes](git-changes.md) and [diffs](git-diff.md).
- [Export and apply patches](git-patches.md).
- [Stage files](git-staging.md) and [undo or redo staging](git-stage-history.md).
- [Create reviewed commits](git-commit.md).
- Manage [worktrees](git-worktrees.md), including [creation](git-worktree-creation.md),
  [existing branches](git-worktree-existing-branches.md), [removal](git-worktree-removal.md),
  [resumption](git-worktree-removal-resumption.md), [recovery](git-worktree-recovery.md) and
  [archive compaction](git-worktree-archive-compaction.md).

## Maintain the project

- [Development](development.md): build, test, add languages and release.
- [Roadmap](../PLAN.md): unfinished product outcomes and acceptance gates.
- [Agent analysis review](agent-analysis-review.md): baseline evidence, analysis gaps and planning architecture.
- [Known defects](../BUGS.md): actionable defects and durable analysis boundaries.
- [Continuity](continuity.md): current contributor handoff.
- [Writing style](style.md): prose rules for user-facing text.
- [Changelog](../CHANGELOG.md): released changes.

Historical design work belongs in Git history and retained evaluation fixtures. The active guides
describe the current product contract.
