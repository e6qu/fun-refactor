# fun-refactor roadmap

`fr` is a deterministic project-understanding and change engine for agents. It gives an agent
bounded semantic structure, exact identities, admitted actions and reviewed delivery without making
the agent ingest a repository as raw text. Source remains available through explicit bounded reveal
when structure is insufficient.

This roadmap contains unfinished product work. Current commands belong in the
[documentation map](docs/README.md), released changes in the [changelog](CHANGELOG.md), and defects
in the [defect ledger](BUGS.md). `fr audit` and `fr capabilities` are the live authorities for
support counts and refusal reasons.

## Product finish line

The product is complete when an agent can:

1. Express an `understand`, `trace`, `change`, `migrate` or `prove` goal without learning the CLI
   command tree.
2. Receive a language-aware route with exact actions, required authored values, result contracts,
   evidence and unresolved questions.
3. Inspect only relevant Merkle-addressed project or semantic subtrees and reveal bounded source
   explicitly when needed.
4. Use the smallest admitted change mechanism: a built-in refactor, recipe, semantic edit, surface
   edit, application migration or proof workflow.
5. Review one complete immutable change, execute only that review, run declared checks, export a Git
   patch and retain undo/redo evidence.
6. Distinguish syntax, compilation, behavioral execution, model theorems and implementation
   correspondence.
7. Receive honest support, uncertainty, omission and refusal data for every operation.
8. Complete the pinned unfamiliar-project acceptance corpus below with live agents across
   representative languages and change shapes. Make no exploratory tool calls after guidance and
   resolve known actionable correctness defects.

Completion excludes guesses about runtime behavior from static source. A translated Lean model
does not prove its source without a checked correspondence boundary.

## Current baseline

The architecture needed for items 1 through 7 exists. Item 8 has deterministic representative
breadth and one accepted guided upstream `understand`/`trace` run. Two accepted matched
source-writing runs still repeat the Rust scalar task. Earlier live agents also made
two-crate changes on the regex workspace through the pre-guide workflow. Autonomous guided
source-writing evidence across task shapes remains narrow. The defect ledger records no open
actionable correctness defect.

| Area | Delivered baseline | Remaining work |
|---|---|---|
| Project understanding | 19 parser identities; symbols, scopes, types, references, calls, flow, impact, entry points, configuration and cross-stack facts | Preserve uncertainty and expand a language only against a concrete task |
| Agent context | Bounded native intent packets, progressive disclosure, Merkle objects, code maps, traces, impact, sources and sinks; local, memory and bounded HTTP stores | Validate on more repositories and storage services |
| Agent routing | Structured goals, ten language-aware guide routes and one uniform writable `GuideReview` lifecycle | Broaden representative acceptance |
| Code changes | Refactors, recipes, semantic bodies and deltas, scalar intents, batches and surface edits | Reduce uneven semantic authoring and translation coverage |
| Application migration | Generic application IR for routes and components; four HTTP adapters write required scalar validation and read their documented source subsets; selected FastAPI middleware, `Depends`/`Security` providers and closed-world service calls; React/Next state and events behind an explicit client boundary | Validation outside each reader subset, configured middleware, provider internals, external services and effects |
| Delivery | Uniform guide review plus checked history, apply, undo, redo, recovery, patches, staging, commits and owned worktrees | Broaden cross-project and cross-language failure testing |
| Formal methods | Lean admission and transition kernels, strict anchors, signature maps, shared cases and external-project proof scaffolds | Strengthen critical implementation correspondence without overstating host guarantees |
| Agent evidence | Deterministic cross-language registry, accepted guided upstream read/trace and three-file rename, pre-guide multi-file upstream cohorts, matched context and two accepted scalar source-writing cohorts | Broaden guided cross-project, cross-language, semantic authoring and proof-writing trials |
| Distribution | Native, WASM and version-matched Python SDK artifacts plus a portable agent skill | Add distribution channels only when consumer demand justifies them |

Run these instead of copying volatile matrices into roadmap prose:

```sh
fr --json audit
fr --json audit workflows
fr --json audit frameworks
fr --json audit proofs
fr capabilities
```

## Bulk delivery roadmap

We group the remaining work into large pull requests that finish an observable product boundary.
Do not split a coherent outcome only to reduce diff size. Each pull request must keep its code,
formal models, tests, fixtures, documentation and retained evidence together. Reviewability comes
from explicit invariants, generated reports and executable acceptance gates.

Dogfood `fr`, its DSL recipes and reviewed flows for repository changes whenever the requested edit
has an admitted route. Inspect the preview, execute the unchanged review and exercise history or
patch delivery. A refusal, incorrect preview or impractical workflow is product evidence: fix it in
the same pull request when its root cause belongs to that outcome. Use a direct editor only when
`fr` has no suitable operation, and retain that boundary in the pull request validation notes.

### Next bulk outcome: representative guided delivery

Use pinned unfamiliar projects and independent oracles to test the complete agent workflow. The
existing representative registry supplies deterministic cases; its two accepted matched live
source-writing cohorts repeat one Rust scalar change. The pinned regex workspace has accepted guided
read/trace and three-file rename tasks. Pinned MIT React/Tailwind and Micromaid projects have accepted
guided TSX class and body, paired Layout/Header bodies, CSS selector and Mermaid node edits.
Earlier live two-crate cohorts used the pre-guide workflow. The paired TSX trial validates one
reviewed two-file authored body delivery. Authored cross-crate Rust bodies remain open. Keep deterministic
replay, live agent outcomes and population claims separate. Pin each task's repository revision,
goal, admitted route, allowed tools, exact postconditions and compiler, framework or proof oracle
before a live run.

The acceptance corpus covers:

- An unfamiliar upstream `understand` and `trace` task with bounded reveal and exact evidence
  (accepted on the pinned regex workspace; retain new failures separately).
- A Rust multi-file change with one complete reviewed delivery. The pinned regex workspace has
  an accepted three-file guided rename; authored cross-crate Rust bodies remain open.
- TSX/React and CSS/Tailwind/Mermaid changes through admitted guides. Pinned React header class,
  standalone CSS selector and authored Layout body edits, plus a Micromaid diagram node edit, are
  accepted. The paired Layout/Header body trial renders three variants and checks both landmarks;
  the CSS selector trial does not establish rendered UI behavior.
- A backend application migration that reads validated inputs from source and writes a different
  HTTP adapter. Preserve manual boundaries and check the generated runtime behavior.
- An agent-authored Lean proof with checked submission and explicit correspondence limits.

Acceptance: retain at least one accepted live run for each task shape above. Source-writing runs
include the guide, preview, checks, apply, undo, redo and patch replay where applicable. The
independent oracle checks the final source or behavior. Retain refusals and failed attempts as
evidence; report model settings, calls and context without extrapolating from the cohort. Fix
actionable product failures exposed by these tasks before marking the outcome complete.

### Following bulk outcome: task-driven application and semantic IR expansion

Expand high-level changes only for concrete failures in the acceptance corpus. All four HTTP
adapters already read their documented validated-input subsets; report other syntax and native
failure semantics as explicit boundaries.

- Extend source-side validation only where the IR can state the same transport, success and failure
  contract.
- Model configured middleware, provider outputs in responses and service-call request bodies. Add
  them only where the IR can state order, inputs, outputs and refusals.
- Expand React/Next rendering beyond declared state and literal events through explicit effect
  semantics; effects remain refusals until then.
- Bring semantic authoring and executable translation to the language constructs required by the
  acceptance corpus.
- Preserve generic adapters; fixtures may use examples, but production recognition and generation
  must not depend on example names or layouts.

Acceptance: each new IR construct has reader/writer compatibility cells, negative cases and
independent round-trip checks. Behavioral claims have runtime fixtures. Delivery is reversible and
the correspondence boundary is explicit.

### Later bulk outcome: critical correspondence and resilience

Reduce the trusted boundary where failure could silently admit stale work, corrupt history or
misstate evidence.

- Extract small executable kernels for canonical serialization, Merkle records, patch planning,
  semantic lowering and history transitions where proofs provide meaningful guarantees.
- Add checked generation or differential correspondence for those kernels.
- Use property, fault-injection and crash-recovery tests for filesystem, Git and subprocess behavior
  that cannot usefully be proved inside Lean.
- Measure and reduce full-gate cost while retaining Lean's default two-job and two-thread bounds.
- Keep proof records explicit about assumptions, trusted tools and unproved host behavior.

Acceptance: strict verification rejects stale anchors, changed signatures, hidden proof debt and
correspondence drift. Recovery and patch tests cover injected interruption points. Default, WASM and
deep gates pass from a clean build.

## Product invariants

- Reads and writes stay inside the selected workspace unless a command explicitly names an external
  read-only toolchain or caller-provided object store.
- Reports state coverage, omissions, confidence, source basis and uncertainty. Truncation never
  becomes absence.
- Source-free evidence remains source-free. Exact source requires an explicit bounded action.
- Every mutation has a complete preview whose basis commits all planning inputs.
- User changes outside selected snapshots survive apply, undo and redo.
- Unsupported or ambiguous inputs refuse before history creation.
- Content-addressed values use one canonical representation and support independent verification.
- Python mirrors public IR shapes. Its package root stays empty, with no `__main__.py` and no mutable
  `__all__`. Python tests use pytest and type-check with `ty`.
- Dependency upgrades use the newest stable release that has been public for at least 24 hours.

## Verification policy

Formalize properties whose failure could silently change code, admit stale work, corrupt history or
overstate evidence. Prefer small executable kernels with explicit assumptions over broad models
without implementation correspondence.

Every proof record names its property, domain, assumptions, Lean declaration, checking toolchain,
source anchors, signature maps, executable bridge, trusted components and remaining obligations.
Shared finite cases establish agreement only for those cases. A general implementation claim needs
a correspondence proof or reviewed verified-generation path.

Use focused tests while implementing, then run the affected complete gates:

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

The default gate covers formatting, lint, native tests, capability coverage, prose and Lean kernels.
The WASM gate covers browser builds and APIs. The deep gate covers repository-scale agreement,
conformance, translation round trips, exhaustive Lean cases and external patch replay. Generated
code also runs its pinned compiler or framework oracle.

Rust test fan-out for the default, deep and Lean-kernel gates defaults to two. Lean worker threads
use the same bound. Builders may set positive `FR_LEAN_JOBS` and `LEAN_NUM_THREADS` values
explicitly; use `1` for both on a resource-constrained workstation.

## Durable boundaries

These are product constraints, not unfinished roadmap items:

- Static analysis cannot settle runtime-generated names, external callbacks, reflection or every
  dynamic dispatch target.
- Package-manager solving belongs to reviewed package-manager commands.
- Framework facts describe static evidence; compiler and runtime commands provide execution
  evidence.
- Browser history handles bounded UTF-8 regular files with projected mode `0644`; native `fr`
  handles executable files, symlinks, staging, commits and worktrees.
- A daemon or watch process remains outside scope until measurement shows that bounded batches,
  caching and concurrent-build coalescing are insufficient.
