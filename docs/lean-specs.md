# Lean specifications with fr

`fr` reads and writes Lean. It checks source anchors and explicit signature maps,
renews reviewed source hashes, and builds the Lean packages that own selected specs.
The [roadmap](../PLAN.md) extends this foundation into an adoption workflow for other projects.

## What exists

| Surface | Current scope |
|---|---|
| Lean translation | Eight programming-language readers and writers, including Lean, over supported constructs |
| `fr spec init` | Preview or create a pinned minimal Lake package and checked `FrSpecs` target through source history |
| `fr spec scaffold` | Select a Rust function and create an anchored, strictly mapped model obligation in that target |
| `fr spec ci` | Generate an undoable GitHub Actions workflow with strict checks, a debt ceiling and a Lake build |
| `fr spec check` | Source identity, missing declarations, signature maps and live `sorry` counts |
| `fr spec check --strict` | Require an explicit signature map beside every source anchor |
| `fr spec check --max-debt N` | Reject a proof-debt increase above a reviewed ceiling |
| `fr spec sync` | Preview renewal of stale source hashes; `--write` applies reviewed renewals |
| `fr spec verify` | Strict correspondence checks, then `lake build --wfail` in each owning package |
| `kernels/` | Executable edit, position, history, pagination, source-budget, insertion-placement, confidence and workspace membership models with shared Rust/Lean cases |

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
fr spec init
fr spec init --write
fr spec scaffold src/lib.rs::allowed
fr spec scaffold src/lib.rs::allowed --write
fr spec ci --max-debt 0 --write

fr spec check --strict
fr spec check --strict --max-debt 0
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
`tools/check-kernels.sh` builds the package with warnings as errors and runs all five executables.
The full self-audits run in `tools/check.sh deep`.

`FrKernels.History` adds snapshot acceptance, inverse laws, mixed-state recovery and undo/redo stack laws.
Its anchored snapshot predicate has 432 shared Rust/Lean executable cases, including a symlink snapshot.
An abstract selected-namespace model proves that replay installs the requested selected snapshot.
It preserves the current snapshot at every unselected path, including an unrelated edit made after application.
The inverse and mixed-recovery proofs use Lean’s propositional extensionality axiom. The two stack inverse proofs use no axioms.
The selected-namespace proofs use no axioms. The model assumes durable journal checkpoints and atomic rename.
Filesystem, path selection and full transaction implementation correspondence remain unproved. A Git-backed CLI test supplies concrete preservation evidence.

`FrKernels.Patch` models Git regular, executable and symlink mode projection, supported permission changes and receiving patch-basis equality.
Five Rust helpers used by file authoring, patch export and receiving checks carry explicit anchors and signature maps.
Mode fields use `UInt32`, matching Rust's `u32` domain, including complement and XOR operations.
The model's 24 theorems establish:

- Projection produces only regular or executable Git modes, depends exactly on the owner-execute bit and is idempotent.
- Snapshot projection fixes symlinks at `120000`, delegates regular entries to the executable projection and distinguishes both kinds.
- Supported mode changes preserve all non-execute bits and either change nothing or toggle owner execute. Reversing a change preserves support.
- Basis matching is reflexive, symmetric and transitive, preserves existence and entry kind, and requires identical content and owner-execute bits for present regular files.
- Full snapshot equality implies patch-basis acceptance. Other permission differences can pass the patch check while failing full snapshot equality.
- The owner-execute setter changes the requested bit, preserves other bits, is idempotent and always produces a supported mode change.
- The setter produces the requested Git mode and preserves the journal's maximum recorded permission value.

Shared execution compares 45,419 mode results across all 4,096 permission patterns, individual high bits, `u32::MAX` and ten change masks.
It also compares 1,849 pairs of absent/present snapshots with empty, Unicode and NUL-containing contents across ten modes plus two link targets.
NUL cases exercise the pure comparison only; patch export still refuses binary snapshots before receiving checks.
Another 8,258 comparisons cover both owner-execute settings, and 8,258 cover both snapshot kinds across the same mode corpus.

An axiom audit of all 24 theorems reports `propext` and `Quot.sound`.
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

`FrKernels.Git` models the inclusive line-range predicate used by changed-declaration views.
Six theorems characterize membership, reject lines before/after or within reversed bounds, characterize singletons, and preserve matches when bounds widen.
Shared execution compares 1,728 cases, including zero, reversed ranges and 32-bit/64-bit maximum values.
The axiom audit reports `propext`, with `Quot.sound` and `Classical.choice` used by some proofs.
These proofs do not add compiler-trust axioms.
The source anchor and signature map identify the Rust predicate; general Rust/model correspondence remains unproved.
Git capture, hashing, syntax extraction, byte-to-line conversion, hierarchy and report aggregation remain outside these laws.

The Git model also anchors the boolean direction predicate used by snapshot call pages.
Six laws characterize empty selections, disabled directions, incoming-only, outgoing-only, both-direction and symmetric selection.
All six proofs use no axioms. Shared execution compares all 16 boolean inputs with Rust.
The predicate receives endpoint membership and direction flags; their derivation, enum mapping, call graph construction and complete Rust correspondence remain unproved.
Explicit call context reuses this predicate and the anchored Git mode projection for working files.
Its selected-file inventory, file-aware containment and snapshot consistency checks have regression evidence, without additional model proofs.

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

## Bounded source kernels

`FrKernels.Source` models the UTF-8 slicing helper that serves `project find --source` and `project show --source`.
Its source anchor and explicit signature map identify `src/project.rs::source_slice_length`.
The model defines byte boundaries as sums of Unicode scalar widths and searches backward for the greatest boundary within the budget.
Offsets and budgets use natural numbers. Shared tests compare cases that the host's `usize` can represent.

Nineteen theorems establish:

- Zero and the source end are boundaries; every boundary lies within the source.
- Slicing accepts exactly valid starting boundaries and refuses other offsets.
- An accepted slice ends at a boundary, respects the byte limit and stays within the source.
- Each slice takes the longest prefix that fits and partitions the remaining byte count.
- A zero budget returns zero bytes. A sufficient budget finishes the source.
- A slice advances when a later boundary fits; a budget smaller than the next scalar can leave an empty slice.
- Page allocation preserves every row, shares one budget, partitions used and remaining bytes, and returns zero lengths after exhaustion.

The shared corpus compares 19,220 slice cases over 90 strings on 64-bit hosts.
It includes every byte offset, split-scalar offsets, source ends, out-of-range offsets, zero budgets and machine limits.
Strings cover one-through-four-byte scalars, Unicode width boundaries, combining marks, NUL, CRLF and escape characters.
An independent Rust oracle accumulates scalar widths forward; the production helper searches backward from the byte cap.
The test also reconstructs the original text around each accepted slice.

Another 5,180 cases compare page allocations over all zero-through-three-row sequences drawn from six strings.
These exercise empty rows, exhausted budgets, partial scalars and unused bytes that a later row can consume.
Eleven CLI comparisons pass actual `find` reports and their selected source through the executable Lean model.
Existing project regressions retain continuation, stale-handle refusal, row pagination and large-body checks.

The axiom audit reports `propext`, `Quot.sound` and, for some proofs, `Classical.choice`.
These proofs add no custom or compiler-trust axioms and contain no `sorry` obligations.
Inspect individual dependencies with `#print axioms FrKernels.Source.page_respects_shared_budget` in a file importing `FrKernels.Source`.
`tools/check-kernels.sh` builds the module and runs both corpora through the existing project executable.

These are model proofs with tested implementation correspondence.
The page model represents the caller's allocation loop; it has no separate source anchor.
General Rust correspondence, UTF-8 library internals, parser spans and JSON report assembly remain unproved.
JSON escaping and metadata lie outside the raw source-text budget.

## Declaration insertion placement kernels

`FrKernels.Author` models the byte offset used to insert a Rust function before an inline module, impl or trait closing brace.
The input string contains the source before that brace; `bodyStart` is the byte offset of the selected body's opening brace.
A line ending with only spaces, tabs or carriage returns keeps its indentation after the inserted fragment.
Otherwise, insertion uses the closing-brace offset. A candidate line must start strictly after `bodyStart`.

The model uses character lists and UTF-8 byte widths. Rust uses a last-newline search and an ASCII byte predicate.
Fifteen theorems establish:

- The result is within the input and on a UTF-8 boundary.
- If the opening brace lies within the input, insertion stays strictly after it.
- The suffix after the insertion point contains only the accepted indentation characters.
- A newline followed by indentation selects that line when it lies inside the body.
- Content at the end retains the closing-brace position; lines outside the body also fall back to that position.
- Empty input yields offset zero.

`src/project.rs::declaration_insertion_offset` holds the shared calculation used by declaration authoring.
It has a source anchor and explicit signature map. The model accepts arbitrary natural opening offsets; Rust accepts `usize` offsets.
The positive opening-bound theorem assumes the opening offset is less than the input's byte length.
The other offset bounds and boundary guarantees hold even for an opening offset beyond the input.

`tests/lean_kernels.rs` compares 28,185 cases on 64-bit hosts against Rust, Lean and an independent reverse-character scan.
The corpus contains all zero-through-four-character words over seven symbols, plus ten special prefixes and three large cases.
It covers CRLF, Unicode, indentation lookalikes, NUL in the pure helper, offsets inside multibyte characters, and machine limits.
Large cases include 65,536 spaces and a prefix containing 4,096 four-byte characters.
A 32-bit host compares 25,371 cases, consuming but skipping opening offsets that its `usize` cannot represent.
The same corpus applies each offset through the Rust edit engine and checks unchanged source prefixes and suffixes.
Ten actual CLI previews across modules, impls and traits also match the Lean placement result, alongside the existing insertion splice and history tests.

All fifteen theorem dependencies use only `propext`, `Classical.choice` and `Quot.sound`, with smaller subsets for some properties.
There are no custom axioms or new obligations. Inspect each dependency with `#print axioms FrKernels.Author.offset_is_boundary`, for example.
Run `cargo test --test lean_kernels declaration_insertion_` for the comparisons.
The default kernel gate includes `lake exe fr-project-kernel declaration-offsets`; the package still uses five executables.

These are model proofs and tested implementation correspondence.
They do not prove AST selection, parser correctness, fragment validity, name checks, filesystem behavior or general Rust/model refinement.
The existing edit model supplies separate splice-preservation laws; byte-offset placement alone does not prove complete authoring correctness.

## Batch selection conflict kernel

`FrKernels.Author.selectionConflict` models the extra selection rule applied before a batch becomes an edit set.
Nonempty half-open regions may be adjacent, but overlapping regions refuse.
An insertion conflicts at either boundary or anywhere inside another selected region, and two insertions conflict at the same offset.
This stricter rule prevents an operation from depending on ordering at a shared original-source boundary.

Six theorems establish symmetry, permit adjacent nonempty regions and distinct separated insertion points, and reject nonempty overlap plus insertion at either boundary.
`src/project.rs::author_selection_conflict` is the shared Rust predicate used by the batch planner and carries a source anchor and explicit signature map.
The generated comparison checks all 6,084 pairs of valid ranges over twelve 64-bit boundary values against Lean and a separate interval oracle.
A 32-bit host compares the 4,356 representable pairs while consuming all Lean output.
Dedicated cases treat the byte boundaries around `é` and `🙂` as insertion, adjacency and overlap positions.

These proofs characterize the numeric selection predicate for valid ranges.
The parser supplies valid byte spans, and separate tests cover malformed manifests, actual duplicate and nested selections, Unicode splicing and refusal before history writes.
They do not prove that parser spans identify the intended declarations or that every batch operation is semantically independent.
The edit kernel separately validates UTF-8 splice boundaries and applies the accepted, disjoint edit set.
Run `cargo test --test lean_kernels author_selection_conflicts_` for the model comparison.

## Revision buffer kernels

`FrKernels.Digest` models the revision buffer as emitted and pending byte lists.
Successful writes append complete serialized fragments; failed writes append a partial fragment and truncate it back to the prior length.
A successful write flushes when pending length reaches the threshold. Explicit flushes move pending bytes to the emitted stream.
Finalization flushes the remainder. These definitions model buffering after serialization supplies bytes, without modeling the serializer itself.

Twenty-one theorems establish:

- Flushing preserves ordered bytes, empties the pending buffer and is idempotent.
- Threshold checks preserve bytes and leave pending length below every positive threshold after a successful write.
- Failed writes restore the prior state and preserve subsequent processing, regardless of partial output size.
- Successful writes append bytes in order; arbitrary operation sequences retain exactly their successful fragments.
- Sequences compose, preserve the pending-length bound and finalize to the initial bytes followed by all successful bytes.
- Changing thresholds or inserting explicit flushes preserves final bytes.
- An abstract incremental digest has the same result across thresholds when its update function obeys the stated chunk-composition law.

The byte laws hold for lists over any element type and thresholds over natural numbers, including zero where no positive bound is claimed.
The digest law assumes `update seed (left ++ right) = update (update seed left) right`.
This is an explicit theorem premise, not a new axiom or a proof about SHA-256 internals.
The axiom audit for all twenty-one theorems reports only `propext` and `Quot.sound`; several need no axioms.

`tests/lean_digest.rs` compiles the same private Rust source module that project construction uses.
It compares 1,570 states across 404 sequences with `fr-digest-kernel` and an independent oracle of explicit JSON bytes.
Four hundred sequences enumerate all zero-through-three-operation combinations from seven operations.
They cover null, booleans, empty strings, Unicode and escaped newlines, the largest unsigned 64-bit integer, partial failures and explicit flushes.
Four longer sequences exercise pending lengths just below, at and above 65,536 bytes, plus a 100,000-character record.
Each includes a failure after writing a 100,000-character partial string, subsequent writes and repeated flushes.
Every state checks exact pending bytes, the digest of emitted bytes, the final digest and the successful-byte oracle.

The Rust comparison uses the production threshold of 65,536 bytes. General threshold laws belong to the Lean model.
Post-operation pending length differs from allocation capacity: a serialized item can exceed the threshold, and Rust retains the largest buffer allocation.
The model does not cover allocation failures, panics, serializer correctness, SHA-256 internals or project revision-input selection.
It has no separate source anchor or proof that Rust refines every model operation; the shared executions establish correspondence on their cases.
The existing twenty-three source anchors retain their separate scope.

```sh
CARGO_HOME="$PWD/target/cargo-home" CARGO_NET_OFFLINE=true cargo test --test lean_digest
```

`tools/check-kernels.sh` builds the model and runs its corpus; the default native gate runs the Rust comparison.
Inspect a theorem's dependencies with `#print axioms FrKernels.Digest.digest_view_preserves_thresholds` in a file importing `FrKernels.Digest`.
The [M4s timing report](project-context-evaluation.md#batched-revision-hashing) remains historical evidence; M4t introduces no timing or context-saving claim.

## Adopting Lean in another project today

Run `fr spec init` to preview a minimal package, then apply it with `--write`.
The command defaults to `specs/`, pins the supported Lean toolchain, and records all
created files in one undoable transaction. Import every selected model from
`FrSpecs.lean` so the default target builds it. The command preserves every existing
package file that differs from its template.

Write a small executable model with a useful property.
Choose a pure function whose domain and assumptions can be stated clearly.
For a supported Rust signature, `fr spec scaffold src/lib.rs::allowed` creates its source
anchor, explicit map, imported module and handwritten model region. Review and apply
that two-file transaction, replace its visible `sorry`, and add the useful theorem.
Other languages and Rust signatures outside the documented type subset still require
manual model and anchor authoring. Then run `fr spec check --strict`.
Run `fr spec verify` to check correspondence and build the owning package.
Add shared input/output cases when the model mirrors an implementation.
Generate CI after choosing the current debt ceiling. The workflow pins this `fr`
release, runs strict correspondence and uses the official
[`lean-action`](https://github.com/leanprover/lean-action) for the initialized package.

Model semantics and proofs remain handwritten work. After source drift, repeat the
scaffold command to preview a new anchor and signature. Refresh changes only the marked
generated region and preserves the complete handwritten region byte for byte. Review
signature changes because preserved model text can still need a type repair.
Use the existing examples under `kernels/` as working references.

## Adoption milestones

The remaining adoption work should produce bounded evidence reports. Those reports
must separate proved models, tested correspondence and proved implementation
correspondence, while naming assumptions, axioms and trusted components.

A `-- fr:debt NAME` line immediately before `sorry` names a live obligation. Strict
checks reject unnamed obligations, and `--max-debt` supplies a CI ratchet. Generated
and handwritten scaffold markers now define regeneration ownership. A separate
kernel-generation annotation remains a proposal.
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
The portable agent skill includes a [Lean reference](../skills/fr/references/lean.md) with an executable anchor-review workflow.

Staging proposals reuse the same anchored Git mode projection and shared snapshot readers as explicit call context.
Staging history adds anchored transition and compaction predicates, each checked against all boolean inputs, plus abstract index and payload laws.
Those laws establish undo/redo round trips, preservation of unselected entries, compaction idempotence and selected-payload removal.
The crash-state predicate classifies every combination of pending journal, index-lock and preparation evidence; Lean proves the clean-state equivalence.
Index locking, journal durability, basis hashing and prepared installation remain outside complete correspondence proofs.
See [staging history assurance](git-stage-history.md#formal-coverage) for assumptions and tested behavior.

Reviewed commits add an anchored branch/parent predicate and abstract publication laws that preserve the index and unrelated refs.
Shared Rust/Lean cases cover branch switches with identical parents, changed parents and unborn states.
See [commit assurance](git-commit.md#formal-coverage) for the Git-locking assumptions and remaining implementation boundaries.

Reviewed worktree creation adds an anchored payload budget predicate and abstract fresh-destination preservation laws.
Shared cases check file, total-byte and per-blob limits at their boundaries.
See [worktree creation assurance](git-worktree-creation.md#formal-coverage) for namespace assumptions and host workflow limits.

Recorded worktree recovery adds an anchored file-acceptance predicate and abstract existing-file preservation laws.
Shared Rust/Lean cases cover every boolean input. Ownership receipts and filesystem durability still require host-level evidence.
See [worktree recovery](git-worktree-recovery.md) for the tested protocol and proof boundaries.

Worktree configuration adds an anchored mode-and-file predicate with proofs that acceptance requires the reviewed repository mode and a regular file whenever `config.worktree` is present.
Rust and Lean agree on all sixteen boolean states. Git configuration parsing, file observation and lifecycle durability remain host-tested assumptions.

Pre-receipt worktree recovery adds an anchored evidence predicate.
Lean proves that recovery rejects an existing receipt, a mismatched preparation or a mismatched registration, and accepts the complete provisional evidence state.
Rust and Lean agree on all eight boolean inputs. Host tests kill creation after Git registration and check receipt publication, preparation cleanup and checkout completion.
They also check existing-branch preservation.

Raw worktree entry support adds an anchored mode policy for regular, executable and symlink blobs.
Lean proves that every accepted entry is a blob with one recognized kind and that ambiguous kinds refuse.
Rust and Lean agree on all sixteen boolean states. Host tests cover symlink creation, interrupted recovery, removal and archive validation.

The Git removal kernel anchors the identity, bytes and mode guard used before deleting reviewed worktree files.
Lean proves that acceptance requires all three matches. Shared executable tests cover all eight input combinations.
An abstract namespace model proves that selected removal preserves other paths.
The host filesystem, Git branch leases and removal archive durability remain outside full correspondence proofs.

Removal resumption adds an anchored predicate for absent paths and matching survivors, with all sixteen boolean cases checked against Lean.
Lean proves absent-path acceptance, required matches for present paths and idempotence of abstract selected removal.
See [removal resumption assurance](git-worktree-removal-resumption.md#formal-coverage) for the host workflow boundaries.

Existing-branch checkout adds an anchored branch-selection guard, checked against every boolean input.
Lean proves that accepted branches are unused and have the presence required by the selected mode.
An abstract attachment law preserves all refs; host tests check the Git lease and lifecycle behavior.
See [existing-branch assurance](git-worktree-existing-branches.md#formal-coverage) for the remaining correspondence boundaries.
