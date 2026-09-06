# Lean specifications with fr

`fr` reads and writes Lean. It checks source anchors and explicit signature maps,
renews reviewed source hashes, and builds the Lean packages that own selected specs.
The [roadmap](../PLAN.md) extends this foundation into an adoption workflow for other projects.

## What exists

| Surface | Current scope |
|---|---|
| Lean translation | Eight programming-language readers and writers, including Lean, over supported constructs |
| `fr spec check` | Source identity, missing declarations, signature maps and live `sorry` counts |
| `fr spec check --strict` | Require an explicit signature map beside every source anchor |
| `fr spec sync` | Preview renewal of stale source hashes; `--write` applies reviewed renewals |
| `fr spec verify` | Strict correspondence checks, then `lake build --wfail` in each owning package |
| `kernels/` | Executable edit, position, history, pagination, confidence and workspace membership models with shared Rust/Lean cases |

Strict signature maps currently require Rust source declarations.
The checker compares both signatures with the explicit map. It does not infer semantic equivalence between mapped types.
A changed source signature remains visible after hash synchronization.

## Source anchors

A spec names the declaration it models and a prefix of its SHA-256 hash:

```lean
-- fr:spec src/edit.rs::apply_to_string @ 3e192284
-- fr:signature source: &str => source: String; edits: &[Edit] => edits: List Edit; return: Result<String> => return: Option String
def applyChecked (source : String) (edits : List Edit) : Option String :=
  if valid source edits then some (apply source edits) else none
```

The hash covers the source declaration's bytes.
The mapping lists source and Lean parameters and return types in order.
Inspect a stale source change before renewing its hash.
`spec sync` changes source identity markers; it does not rewrite signatures or repair proofs.

```sh
fr spec check --strict
fr spec sync
fr spec sync --write
fr spec verify
```

Without paths, these commands inspect existing `kernels/` and `specs/` roots.
Pass a Lean file or directory to select another location.
`verify` requires each selected file to belong to a Lake package.
Lean is an explicit dependency for verification, not for ordinary refactoring.

## What each check establishes

A fresh anchor establishes that the named source bytes match the recorded identity.
An accepted signature map establishes correspondence with the two declared signatures.
A Lean theorem establishes its proposition under its definitions and assumptions.
A shared execution test compares the implementation and model on the selected cases.

A theorem about a Lean model alone does not prove the Rust implementation refines that model.
Translation into Lean does not supply that proof either.
Implementation correspondence needs its own argument or a justified verified generation path.
Keep assumptions, accepted axioms and trusted components visible in any verification report.

## Existing kernels

`FrKernels.Edit` models byte-based edits, UTF-8 boundaries, ordering, overlap checks and splice application.
It states properties of accepted and rejected plans and unchanged source prefixes.
`FrKernels.Position` models line and column conversion and full-line spans.

`tests/lean_kernels.rs` compares the executable models with Rust over ASCII and Unicode corpora.
It also checks plans from real refactoring commands.
`tools/check-kernels.sh` builds the package with warnings as errors and runs all four executables.
The full self-audits run in `tools/check.sh deep`.

`FrKernels.History` adds snapshot acceptance, inverse laws, mixed-state recovery and undo/redo stack laws.
Its anchored snapshot predicate has 250 shared Rust/Lean executable cases.
The inverse and mixed-recovery proofs use Lean’s propositional extensionality axiom. The two stack inverse proofs use no axioms.
The model assumes durable journal checkpoints and atomic rename. Filesystem and full transaction implementation correspondence remain unproved.

`FrKernels.Patch` models Git executable-mode projection, supported permission changes and receiving patch-basis equality.
Three Rust helpers used by patch export and receiving checks carry explicit anchors and signature maps.
Mode fields use `UInt32`, matching Rust's `u32` domain, including complement and XOR operations.
The model's 14 theorems establish:

- Projection produces only regular or executable Git modes, depends exactly on the owner-execute bit and is idempotent.
- Supported mode changes preserve all non-execute bits and either change nothing or toggle owner execute. Reversing a change preserves support.
- Basis matching is reflexive, symmetric and transitive, preserves existence, and requires identical content and owner-execute bits for present files.
- Full snapshot equality implies patch-basis acceptance. Other permission differences can pass the patch check while failing full snapshot equality.

Shared execution compares 45,419 mode results across all 4,096 permission patterns, individual high bits, `u32::MAX` and ten change masks.
It also compares 1,681 pairs of absent/present snapshots with empty, Unicode and NUL-containing contents across ten modes.
NUL cases exercise the pure comparison only; patch export still refuses binary snapshots before receiving checks.

An axiom audit of all 14 theorems reports `propext` and `Quot.sound`.
Proofs using `bv_decide`, and theorems depending on them, also use `Classical.choice`, `Lean.ofReduceBool` and `Lean.trustCompiler`.
Lean 4.28's [bitvector proof checker](https://github.com/leanprover/lean4/blob/v4.28.0/src/Lean/Elab/Tactic/BVDecide/Frontend/BVDecide.lean) performs compiled certificate validation.
That adds compiler trust to those proofs. Zero `sorry` obligations does not remove these assumptions.
Inspect individual dependencies with `#print axioms FrKernels.Patch.mode_change_supported_iff` in a Lean file importing `FrKernels.Patch`.
Source anchors and shared cases do not prove general Rust/model correspondence.
Filesystem observation, path validation, patch rendering, report aggregation and Git execution remain outside this model.

`FrKernels.Project` models the shared page-length calculation and workspace component matcher.
Its theorems bound each page by the requested limit and remaining items.
They also prove forward progress and partition the remaining result set.
The executable corpus includes 1,728 combinations, with 32-bit and 64-bit integer limits.
Rust compares all cases its `usize` can represent.
The matcher proves equal directory depth, refusal at different depths, self-matching and literal-or-star head matching.
Its shared corpus compares 67,081 pairs of component sequences, including Unicode, empty strings and embedded slash characters.
The caller splits paths into components and restricts pattern syntax before matching; those parsing steps remain outside the proof.
The model does not establish filesystem containment or package-manager workspace membership.
Matcher proofs use propositional extensionality; the self-match proof also uses Lean's standard classical-choice and quotient-soundness axioms.
The model does not prove parser correctness, snapshot-hash collision resistance or agent task success.

The project kernel also models path confidence as the maximum of edge ranks, with zero as the empty-path identity.
Ranks map `exact`, `import-qualified`, `field-based` and `name-only` to 0 through 3, in that order.
Theorems show that aggregation cannot strengthen any input edge and stays within the supplied tier bound.
The compact test view leaves path confidence null for in-scope candidates, which have no witness edges.
All 5,461 rank sequences through six edges agree with the Rust helper, including the empty sequence.
The non-strengthening and tier-bound proofs use propositional extensionality; the empty-path proof uses no axioms.
These laws concern aggregation of supplied edges. Catalog accuracy, graph construction and shortest-path correspondence remain outside these proofs.

The workspace membership kernel models one synchronous expansion over supplied package IDs and eligible dependency edges.
The Rust workspace reader uses this anchored helper after capturing ownership, exclusions and local dependency evidence.
The model proves that a step preserves existing members, adds exactly targets of edges from existing members, and is monotone.
Repeated expansion preserves seeds and adds only reachable nodes. Every round stays within any closed superset of the seeds.
`FrKernels.Workspace` proves general convergence: expansion stabilizes within the number of supplied dependency edges.
The argument covers arbitrary finite lists of IDs, seeds and edges, including duplicates, cycles and disconnected components.
Each changing round removes at least one entry from the finite list of missing candidates.
Sorted, duplicate-free representation makes equal membership imply the list equality used by the stopping condition.
The executable closure model runs to the proved bound and returns exactly the reachable nodes, hence the least closed superset of the seeds.
Every later round returns the same list. These conclusions require no separate stabilization hypothesis.

Shared execution compares 20,750 rounds across every directed graph and seed set on zero through three nodes.
An independent Rust queue traversal checks final reachability and stabilization within the node count for those cases.
Four further shared rounds cover duplicate seeds/edges, unsorted IDs and 64-bit limits on hosts that can represent them.
CLI regressions retain exclusions, distinct owners, cycles, inherited paths and deterministic first-round witnesses.
Closure comparisons cover all 4,165 small graph/seed configurations, a 64-bit duplicate/limit case and chains of 4, 16 and 64 edges.
The chains require exactly their edge count in changing Rust rounds, exercising the bound without an early-stop assumption.
The reachability induction uses no axioms; the expansion proofs use propositional extensionality and quotient soundness from Lean's standard library.
The convergence and unconditional closure proofs also use Lean's standard classical-choice axiom. These proofs introduce no custom axioms.
These are model proofs with tested Rust correspondence. Cargo semantics, eligible-edge construction, witness selection and the complete Rust loop remain outside the proofs.

The Cargo reader checks literal exclusion prefixes and explicit-member overrides with `cargo metadata` fixtures.
These cover nested roots, descendant dependencies, Unicode paths and neighboring directory names.
The [Cargo implementation](https://doc.rust-lang.org/stable/nightly-rustc/src/cargo/core/workspace.rs.html) uses directory prefixes for exclusions and lets explicit member paths override them.
Glob-shaped exclusions remain outside the reader's supported subset and produce unresolved rows.
These fixtures test rule interpretation; they do not extend the Lean proof boundary.

Cargo member patterns also support leading parent components within the selected project root, followed by the existing fixed-depth pattern subset.
Matching reads captured manifests only. Explicit workspace pointers can establish ownership for sibling packages and their transitive inherited path dependencies.
Pattern-candidate pages retain their separate ownership gaps, even when membership pages have enough evidence.
Declared literal paths retain parent components for exclusion precedence; normalizing such aliases would incorrectly override some Cargo exclusions.
Snapshot escapes, parent components after a literal or wildcard, parent-relative exclusions and npm parent patterns remain unsupported.
Cargo metadata fixtures check sibling membership, inheritance and the alias/exclusion interaction. Path interpretation and ownership still remain outside the Lean proofs.

## Adopting Lean in another project today

Create a Lake package and write a small executable model with a useful property.
Choose a pure function whose domain and assumptions can be stated clearly.
Add its source anchor and explicit signature map, then run `fr spec check --strict`.
Run `fr spec verify` to check correspondence and build the owning package.
Add shared input/output cases when the model mirrors an implementation.

This workflow still requires manual model and anchor authoring.
Package initialization and `spec extract` are planned commands; they do not exist today.
Use the existing examples under `kernels/` as working references.

## Adoption milestones

The next adoption work should provide:

- Package initialization with a pinned Lean toolchain and CI instructions.
- Declaration selection and anchored model scaffolds with explicit unsupported types.
- Named proof obligations and a proof-debt ratchet.
- Generated-region ownership and regeneration that preserves handwritten work.
- Explicit signature synchronization that exposes affected proofs.
- Reports separating proved models, tested correspondence and proved implementation correspondence.

`SPEC-DEBT`, generated-region markers and the kernel-generation annotation remain proposals.
A zero `sorry` count describes the selected files, not the completeness of their specifications.
Reject unapproved axioms and expose assumptions before claiming stronger coverage.

## Formalization order

Extend the edit and position models with general laws that their callers need.
Extend transaction correspondence beyond the snapshot predicate and test storage failure boundaries.
Define an executable IR semantics for a small subset, then prove selected lowerings against it.
Arithmetic, precedence, capture avoidance and scope lookup are useful initial targets.
Expand the subset only with explicit semantics and regression evidence.

A `def` contains executable content. A `theorem` states a proposition and provides no application implementation.
The existing Lean reader translates supported executable constructs into the code IR.
Future generation must retain this distinction and report unsupported definitions.

## Agent responsibilities

The tool identifies drift, enumerates obligations and runs the checker.
An agent chooses useful claims, writes models and searches for proofs.
It must report a false claim rather than weaken that claim to obtain a successful build.

The local [Lean skill](../.claude/skills/lean-spec/SKILL.md) describes the implemented workflow.
The broader agent skill package remains part of [PLAN.md](../PLAN.md).
