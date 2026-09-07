# Handing fr to an agent

The portable [fr skill](../skills/fr/SKILL.md) teaches bounded project exploration and reviewed code changes.
Its references separate exploration, authoring, built-in changes, recipes, checks, history, patches, Git administration and Lean.
The entrypoint routes to those references only when the task needs them.

## Distribution and use

The native release packaging includes `skills/fr/` beside the binary.
From a source checkout, use that same directory.
Copy the whole `fr` skill folder into the receiving agent's skill location, or point the agent at its `SKILL.md` directly.
Keep the `references/` directory beside the entrypoint; all skill links stay inside the portable folder.
Packaging does not install the skill globally or alter the agent's configuration.

Make the matching `fr` binary available to the agent. Ordinary exploration and source history work without Git or Lean.
Git commands need Git; strict model verification also needs the project's Lean/Lake toolchain.
Use the target project's root and existing task authorization.
A request to inspect or export does not grant permission to commit or publish.

Known names can go directly to `project find`; the initial map remains available when the hierarchy needs inspection.
The default lookup and introductory map each request at most twelve rows.
Subsequent queries select a declaration, read its signature and relationships, and request source slices only as needed.
Body authoring accepts project handles; built-in refactorings use names or source positions.
It uses saved transaction IDs for exact plan application, and keeps source history separate from Git bases and journals.
History and check commands do not require refreshing project handles after every write.
The authoring reference avoids loading recipe vocabulary for a body edit; patch guidance loads Git administration only when needed.
The check example uses `--quiet-success` and retains diagnostics for failed commands.
History writes use `--no-diff` after reviewing the saved plan or transition preview, retaining completion metadata without repeated diff text.
The introductory lookup guidance distinguishes a full name from a fragment, which needs `--contains`.

## Executable evidence

Run the examples against a chosen build, including a downloaded release binary:

```sh
python3 tools/check-agent-skill.py --fr /path/to/fr
```

The checker uses temporary projects and executes every fenced shell example from the skill files.
It obtains real handles, positions and transaction IDs from command output, then substitutes them into the example commands.
The source fixture checks:

- Bounded declaration lookup, selected signature, relationships, source slice, call edges, candidate tests and coverage gaps.
- A recipe preview with file-count expectations, then a saved two-file Python rename.
- Refusal of a stale plan and an old project handle after a source change.
- Applied behavior, undo/redo and preservation of an unrelated later edit.
- Exported patch application in a separate Git receiver, with both indexes unchanged.

The Lean fixture checks a stale source anchor, reviewed hash synchronization and a real Lake build.
It then changes the theorem to a false proposition and requires verification to fail with one JSON report.
Separate JSON regressions cover missing declarations and invalid signature maps.
The checker does not exercise interrupted-write recovery or every optional Git operation in the prose; those retain their dedicated CLI regressions.

The standard native test gate runs this checker through `tests/agent_skill.rs`.
The skill validator also checks its frontmatter and unfinished placeholders during authoring.
The checker enforces a 3 KiB entrypoint budget, a 6 KiB budget per reference and valid links inside the portable folder.

The initial macOS run against the development binary executed 31 fenced command examples.
Declared project-check listing and execution now bring the checker to 33 examples.
The initial measurements were:

| Measure | UTF-8 bytes |
|---|---:|
| Skill entrypoint | 2,120 |
| All five optional references | 11,680 |
| Exploration command output | 11,223 |
| Fixture Python source | 66,735 |

These initial figures describe a synthetic project with deliberately unrelated background source.
They are not model token counts or evidence of autonomous agent success.
Path lengths and later documentation changes can change the byte counts; the checker prints fresh measurements.
The native packaging change has a local archive check; release uploads and other platform builds require their normal release jobs.

The [first real-agent evaluation](agent-acceptance.md) now records two fr tasks and their ordinary-file comparisons on a pinned public Rust release.
Both fr tasks pass independent behavioral oracles, declared checks and reversible patch workflows without human corrections.
The [follow-up evaluation](agent-context-followup.md) reduces fr context by 34.0% and 24.5% against its first trials.
The fresh file trials also improve and still use less context. The evidence supports these workflows while leaving broader efficiency open.
The same report includes a subsequent controlled comparison of smaller history completion reports; this adds no new autonomous-agent result.

## Remaining roadmap

M4a provides the introductory handoff and executable command examples.
Body replacement supports Rust, TypeScript and TSX declarations and methods, plus direct TypeScript/TSX function bindings with block bodies.
Rust function declaration replacement can change signatures and implementations together, preserving the name and outer attributes.
Declaration insertion appends a Rust function through a file handle, retaining all existing source bytes.
It accepts leading outer documentation comments, so agents can satisfy a project's missing-docs lint without changing crate policy.
Declared project-check selection now has a configuration digest and bounded execution reports.
Broader real-agent evaluation, context optimization and further authoring operations remain open.
M5 still owns automated Lean package initialization and model scaffolding.
The skill does not claim complete framework migration, worktree undo/redo or general implementation verification.
