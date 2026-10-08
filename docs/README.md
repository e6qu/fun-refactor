# Documentation

Start with the [project README](../README.md) to install `fr` and run a first command. Use this page
to choose one authoritative guide for the task; detailed contracts link to related material instead
of repeating it.

For ongoing repository work, read the [plan](../PLAN.md) for priorities, the
[handoff](continuity.md) for the active collection, and the
[development guide](development.md#working-on-a-shared-desktop) for workstation limits.

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
- [Checked task outcomes](checked-outcomes.md): gate delivery and retained plan completion on post-change behavior checks.
- [Consumer scopes](change-scopes.md): discover bounded callers and test candidates, associate declared checks and bind reviewed changes to fresh scope evidence.
- [Structural changes](structural-changes.md): scope-bound Rust rename and scalar signature delivery.
- [Model comparisons](model-comparisons.md): retained old/new Boolean models, explicit relations and checked proof resumption.
- [Python runtime](agent-runtime-sdk.md): keep intermediate reports outside the model transcript.
- [Context workspace](agent-context-workspace.md): materialize and cache selected Merkle subtrees.
- [Context protocol](agent-context-protocol.md): packet, continuation and review identities.
- [Progressive disclosure](progressive-disclosure.md): reveal semantic, project and source branches.
- [Completion audit](completion-audit.md): inspect live support and trust boundaries.
- [Evaluation evidence](evaluations.md): reproduce and interpret deterministic and live-agent runs.
- [Codex runner](agent-codex-runner.md): run optional paid live-agent evaluations.
- [Unknown-target trials](unknown-target-investigations.md): compare discovery and resumed delivery on pinned upstream tasks.

For a change, the normal path is
`audit → goal → guide → bounded evidence → preview → reviewed execution → checks → patch`.
The [goal guide](agent-workflow-guide.md) owns this contract; the other pages explain individual
stages.

## Understand and change code

- [Semantic model](semantic-model.md): source-free bodies, changes and scalar intents.
- [Expression control](expression-control.md): ordered Python scalar flow, short-circuit calls and analysis boundaries.
- [Python call binding](call-binding.md): parameter kinds, ordered arguments and versioned contracts.
- [Literal defaults](literal-defaults.md): immutable default binding and exact typed signature origins.
- [Scalar assignments](scalar-assignments.md): ordered unpacking, swaps and chained local writes.
- [Namespace packages](namespace-packages.md): static single-root imports and typed directory dependencies.
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
- [CI latency and coverage](ci.md): test shards, complete coverage and runner time limits.
- [Roadmap](../PLAN.md): unfinished product outcomes and acceptance gates.
- [Technical status](roadmap-status.md): what the recorded acceptance tests establish and what remains.
- [Product review](product-review.md): efficiency criteria, scope and removal decisions.
- [Agent analysis review](agent-analysis-review.md): baseline evidence, analysis gaps and planning architecture.
- [Known defects](../BUGS.md): actionable defects and durable analysis boundaries.
- [Continuity](continuity.md): current contributor handoff.
- [Writing style](style.md): prose rules for user-facing text.
- [Changelog](../CHANGELOG.md): released changes.

Historical design work belongs in Git history and retained evaluation fixtures. The active guides
describe the current product contract.

[Investigation evidence and resumable plans](agent-investigations.md) describes exact occurrences,
Python scalar flow, dependency invalidation and checked investigation delivery.

## Evaluate agent use

- [OpenCode tools](opencode-native-tools.md): configured collection, resource admission and frozen replay.
- [Candidate review status](candidate-review-status.md): admitted tasks, failed reviews and remaining gaps.
- [Independent study](agent-study.md): task plans, parent/child accounting and failure-preserving reports.
- [Evaluation evidence](evaluations.md): measured outcomes and historical limits.
- [Evidence restoration](../tests/agent-eval/EVIDENCE-ARCHIVES.md): recover archived results exactly.

Current priorities belong in the plan. Commands belong in the relevant guide. Detailed run histories
and immutable results belong with their evidence; do not copy them into the contributor handoff.
