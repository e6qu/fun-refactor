# fun-refactor roadmap

`fr` is a deterministic tool for agents to understand and change projects through bounded,
high-level data. Agents should consume semantic structure, relationships, identities and exact
actions before requesting source. Every write must remain reviewable, reversible and tied to its
planning evidence.

This document tracks the finish line and remaining product boundaries. The
[documentation map](docs/README.md) covers current usage. The [changelog](CHANGELOG.md),
[defect ledger](BUGS.md), [continuity handoff](docs/continuity.md) and Git history preserve completed
work.

## Finish line

The implementation is complete when an agent can:

1. Express a bounded structured goal without knowing the command tree, recipe grammar, semantic IR
   or proof protocol.
2. Receive a language-aware sequence of exact actions with required inputs, result schemas,
   established evidence and unresolved questions.
3. Inspect only relevant Merkle-addressed project and semantic subtrees, with explicit bounded source
   reveal when structure is insufficient.
4. Select the smallest admitted route among direct refactoring, recipes, semantic edits, framework
   migration and formal proof work.
5. Preview a complete change, retain one review identity and execute only that unchanged review.
6. Run declared checks, export a Git patch, and undo or redo the source transaction.
7. Distinguish model proofs, implementation correspondence, behavioral tests, compiler checks and
   syntax checks.
8. Obtain honest support, uncertainty and refusal reports for every language, framework and
   operation.
9. Complete representative unfamiliar-project tasks without exploratory help calls after the guide
   response and without known actionable correctness defects.

Completion does not include guessing runtime behavior from static source or claiming source
properties from a translated Lean model without a checked bridge.

## Current state

The project completed the original numbered delivery roadmap and merged PR 314. Current work adds
complete reviewed guide-run execution and the first accepted matched source-writing cohort.

| Measure | Current value |
|---|---:|
| Parsed languages | 19 |
| Query sets | 17 |
| Entry-point catalogs | 10 |
| Capability and language cells | 456 |
| Supported cells | 311 |
| Fixed defects | 775 |
| Open defects | 0 |

The remaining 145 cells refuse or are inapplicable with reasons. `fr capabilities` is the authority
for operation support. A supported cell can still refuse an input outside its syntax, identity,
confidence or effect contract.

### Delivered system

- Syntax, symbol, scope, type, reference, call, flow, impact, entry-point and configuration analysis.
- Project models for packages, dependencies, applications, routes, contracts, schemas, tests,
  styles, Markdown and Mermaid.
- Checked refactoring, recipes, declaration authoring, semantic bodies, typed deltas, scalar intents
  and multi-file batches.
- Shared application IR and bounded Next.js, React, FastAPI, Express and Go transformations.
- Merkle-addressed progressive disclosure for semantic IR, maps, traces, impact, source, sinks and
  continuations.
- Structured `understand`, `trace`, `change`, `migrate` and `prove` goals over an immutable snapshot.
- A zero-dependency Python SDK that mirrors the wire IR and keeps intermediate reports local.
- A reviewed `GuideRun` delivery path that refreshes guidance and executes its sole unchanged task
  review through checks, reversal and patch export.
- Recoverable history, apply, undo, redo, compaction, Git patches, staging, reviewed commits and
  owned-worktree operations.
- Declared checks with reviewed configuration digests and bounded diagnostics.
- Lean kernels for critical admission and transition rules, strict source anchors, signature maps,
  proof-debt checks and shared Rust, Python and Lean cases.
- An external-project proof workbench with source-free property plans, bounded goal disclosure,
  agent-authored tactics, checked writes and generated CI.
- Native, WASM and browser interfaces with shared transaction and patch behavior.
- A portable skill, completion audit and guide runtime for low-context agent use.

### Language and framework scope

The parser identities are Rust, Go, Zig, Java, TypeScript, TSX, Python, Bash, HTML, CSS, SCSS, Sass,
HCL, JSON, YAML, Helm, XML, Markdown and Lean. JavaScript uses the TypeScript grammar, while JSX uses
TSX. React, Next.js, Express.js, FastAPI and Tailwind CSS are framework surfaces.

| Layer | Implemented scope | Boundary |
|---|---|---|
| Structural analysis | All 19 parser identities | Dynamic and external behavior remains uncertain |
| Capability operations | 311 checked cells | 145 cells refuse or are inapplicable with reasons |
| Semantic body authoring | Rust, Go, Java, Python, JavaScript, TypeScript and TSX | Other writers use narrower structural operations |
| Executable translation IR | Rust, Go, Java, Python, TypeScript, Zig, Bash and Lean | Unsupported constructs carry explicit gaps |
| Framework evidence | React, Next.js, Express.js, FastAPI, CSS/Tailwind and Markdown/Mermaid | Checked migration focuses on admitted route and static JSX behavior |
| Generated formalization | Typed pure declarations and structural snapshots | Dynamic, effectful and unsupported numeric semantics need separate models |

## Work that remains

1. **Broaden checked semantics.** Extend formalization only with reviewed models and correspondence
   evidence for each new async, dynamic, effectful or numeric construct.
2. **Strengthen implementation correspondence.** Reduce the trusted parser, extraction, lowering,
   serialization and hashing boundaries where small executable kernels or generated bridges can
   carry meaningful guarantees.
3. **Expand framework behavior carefully.** Add request bodies, query validation, middleware and
   authentication only with stated application IR semantics. Apply the same rule to service calls
   and dynamic rendering.
4. **Reduce uneven language support.** Bring semantic authoring, call analysis, translation,
   migration and formalization to more languages without weakening refusal behavior.
5. **Repeat live-agent evidence.** The first matched source-writing pair now passes with exact source,
   compiled behavior and full guided lifecycle evidence. Repeat across other projects, models,
   languages and multi-file tasks. Keep fixture results separate from general context or quota claims.
6. **Preserve static uncertainty.** Continue reporting indirect calls, reflection, generated names
   and unresolved dispatch rather than treating missing static edges as absence.

Choose the next large pull request from these boundaries. Each proposal must define its admitted
subset, refusal surface, behavioral oracle, reversible lifecycle and proof or test obligations before
implementation.

## Product invariants

- Reads and writes stay inside the selected workspace unless a command names an external read-only
  toolchain or object store.
- Reports state coverage, omissions, confidence, source basis and uncertainty. Truncation never
  becomes absence.
- Source-free evidence remains source-free. Exact source needs an explicit bounded action.
- A mutation has a complete preview whose basis commits every planning input.
- User changes outside selected snapshots survive apply, undo and redo.
- Syntax, compilation, behavior, model proofs and implementation correspondence remain separate
  evidence classes.
- Unsupported or ambiguous inputs refuse before history creation.
- Every content-addressed value has one canonical representation and independent verification.
- Python mirrors public IR shapes. Its package root stays empty, with no `__main__.py` and no mutable
  `__all__`. Python tests use pytest and type-check with `ty`.
- Dependency upgrades use the newest stable release after it has been public for at least 24 hours.

## Formal verification policy

Formalize properties whose failure could silently change code, admit stale work, corrupt history or
overstate evidence. Prefer small executable kernels with explicit assumptions over broad models
without implementation correspondence.

Every proof record names its property, domain, assumptions, Lean declaration, checking toolchain,
source anchors, signature maps, executable bridge, trusted components and remaining obligations.
Shared cases prove correspondence only for those cases. A general implementation claim requires a
correspondence proof or a reviewed verified-generation path.

Strict verification rejects stale anchors, changed signatures, missing mappings, unbuilt packages,
warnings, hidden placeholders and proof debt above its reviewed ceiling.

## Validation policy

Use focused tests during implementation, then run the affected complete gates:

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

The default gate covers formatting, lint, native tests, capability coverage, prose and Lean kernels.
The WASM gate covers browser feature builds and APIs. The deep gate covers repository-scale
agreement, conformance, round trips, exhaustive Lean cases and external replay. Python uses pytest
and `ty`; generated code also runs its pinned language or framework compiler.

Lean fan-out and worker threads default to two. Builders can set positive `FR_LEAN_JOBS` and
`LEAN_NUM_THREADS` values explicitly.

Routine autonomous evaluations use the weakest economical model available to the installed Codex
CLI at its lowest supported reasoning effort. Record the CLI version, model, effort, service tier,
authentication mode and usage fields. Keep infrastructure failures separate from agent failures.

## Explicit boundaries

- `fr` is standalone and does not require a language server or daemon.
- Static analysis cannot settle runtime-generated names, external callbacks, reflection or every
  dynamic dispatch target.
- Package-manager solving remains delegated to reviewed package-manager commands.
- Framework facts describe static evidence; compiler and runtime commands provide execution evidence.
- Browser history handles bounded UTF-8 regular files with projected mode `0644`. Native `fr`
  handles executable files, symlinks, staging, commits and worktrees.
- A daemon or watch process remains outside scope until a measured workload shows that bounded
  batches, caching and concurrent-build coalescing are insufficient.
