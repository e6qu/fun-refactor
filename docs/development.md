# Development guide

`fr` requires stable Rust, Python 3 and the toolchains used by the language fixtures. Lean tests
use the repository's pinned Lake project. Zig and Go tests keep their caches under `target` by
default.

## Build and test

Build the native CLI with the locked dependency graph:

```sh
cargo build --locked
cargo run --locked --features cli -- --version
```

The check script defines the same gates as CI. The default and WASM lanes run on pull requests.
The deep lane runs repository-scale audits, every Lean case and external consumer replays.

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

Rust test fan-out in the default, deep and Lean-kernel gates defaults to one. Lean processes use the
same worker bound. Override either value with a positive integer when the machine has suitable
capacity.

```sh
FR_LEAN_JOBS=1 LEAN_NUM_THREADS=1 tools/check.sh default
FR_LEAN_JOBS=1 LEAN_NUM_THREADS=1 tools/check.sh deep
```

Install and check the Python SDK in its own environment:

```sh
python3 -m venv sdk/python/.venv
sdk/python/.venv/bin/python -m pip install -U pip
sdk/python/.venv/bin/python -m pip install -e 'sdk/python[test]'
sdk/python/.venv/bin/python -m pip install 'ty==0.0.80'
sdk/python/.venv/bin/pytest sdk/python/tests
sdk/python/.venv/bin/ty check sdk/python/src
```

Use targeted tests while developing, then run the complete affected lane before merging. The
portable agent skill has an executable example checker:

```sh
cargo test --test agent_skill
python3 tools/check-agent-skill.py --fr target/debug/fr
```

## Working on a shared desktop

Use remote runners for full builds, complete gates and evidence regeneration when local capacity
is constrained. `tools/refresh-refinement-evidence.py` refuses local execution and records the six
source-bound reports affected by the Boolean comparison feature, plus its new acceptance run.
The manual `Refresh refinement evidence` workflow uploads results from a directory unique to each
run and attempt under the runner's temporary directory. Reports stay outside the compiler cache.
Supply a dated `prefix` such as `2026-09-28-virtual` to retain new report directories.
The default `all` group checks virtual workspace boundaries before measuring seven evidence groups.
The `flow` group runs package and SDK boundary tests, then refreshes seven flow reports and index measurements.
Index-resolution evidence binds every Rust source file and must be refreshed after any Rust source change.
Choose one group to refresh only its measurements. Update audit-test report paths before refreshing:
host-recovery evidence includes its audit test's source identity.
Retain downloaded evidence before requesting the final CI run; never substitute rewritten hashes
for fresh measurements.

One compiler worker does not bound total memory, CPU or accumulated build output. For this project,
local workloads must run serially at low priority, with monitored CPU throttling and aggregate RSS.
Stop at 1 GiB sampled workload RSS, 2 GiB under `target`, or less than 64 GiB free disk.
Use short command deadlines and check descendant processes on interruption. Sampling can miss brief
peaks or short-lived children; it is a guard for bounded local work, not an operating-system quota.
Keep broad indexing and compiler-heavy checks on runners. Preserve source and the working executable
when removing rebuildable caches, and never rebuild automatically after cleanup.

## Repeated consumer queries

`Index::references_to` retains extraction order and expands the current definition group.
`reference_count` and `has_references` answer counts and presence without collecting occurrence records.
The first target lookup builds an in-memory reverse index; later queries reuse it.
Name matching builds a separate lookup only when needed. These caches add memory proportional to the reference count.
They do not persist to disk or change resolution confidence, missing consumers or analysis coverage.

`Index::references` now uses `index::References`, which dereferences to the reference vector.
Every mutable borrow discards both derived lookups before exposing data. Rust borrowing prevents
mutation while query results remain borrowed. Cloning or deserializing starts without derived lookups.
Element access, vector methods and borrowed iteration retain their syntax. Replace a whole vector
with `index.references = values.into()` and extract ownership with `index.references.into_vec()`.
Serialization retains the vector representation. Structural vector edits still require callers to
maintain file reference offsets, just as before; this wrapper maintains only consumer lookups.

The [consumer workload](../tests/agent-eval/index-consumers/task.json) measures a pinned repository
snapshot and generated Python definition groups. Compile `cargo test --test index_consumers --no-run`,
then pass the printed test executable to `tools/index-consumers-acceptance.py --binary EXECUTABLE --output RESULT`.
Use the same unoptimized profile for baseline comparisons. The evaluator runs one child at a time,
limits each workload to 240 seconds and records process RSS through `/usr/bin/time`.

## Source provenance

Dependency upgrades use the newest stable release that has been public for at least 24 hours.

Tree-sitter grammars are pinned Cargo dependencies. Repository query files live under `queries/`,
and [their README](../queries/README.md) records conventions. Vendored source and licence details
live in the [vendor guide](../vendor/README.md). Do not download parsers or grammars at runtime.

The agent evaluation fixtures under `tests/agent-eval/` retain their own source, licence and digest
records. See the [evaluation guide](evaluations.md) before updating a retained artifact.

## Add or extend a language

1. Add or update the parser identity and extension mapping.
2. Add the tree-sitter grammar and its provenance.
3. Define symbol, scope and reference queries for the admitted syntax.
4. Add capability predicates with a reason for every unsupported cell.
5. Add fixtures that exercise accepted syntax, ambiguous cases and refusals.
6. Add writer or transformation support only for constructs the IR can represent.
7. Regenerate and check the capability matrix.
8. Update the durable language references and the defect ledger.

The capability matrix reports predicate support. Test coverage separately proves that fixtures
executed each advertised cell.

## Release

Release Please prepares the version and changelog pull request. A version tag starts the release
workflow, which builds native archives and the WASM package, attaches checksums and verifies the
published shapes. Keep installation instructions independent of a specific version number.

Before release, run the default, WASM and deep lanes and inspect the generated archive locally.

```sh
cargo test --test release
cargo test --test packaging
```

## Resolution ambiguity checks

Resolution asks whether a second entity remains after the first definition group.
It skips that question when lexical lookup already permits a decision. Full entity counts
still follow input order; definition groups can be asymmetric after public symbol mutation.
Neither operation retains derived symbol groups across calls. During resolution, the complete name map
also rejects absent receiver types without a full-symbol scan. Type queries outside that immutable
pass retain their fallback for public symbol mutations.

The [resolution workload](../tests/agent-eval/index-resolution/task.json) pins complete symbol and
reference outputs before the algorithm change. Build its probe with one worker:

```sh
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_PROFILE_DEV_DEBUG=0 \
  cargo test --test index_resolution --no-run
python3 tools/index-resolution-acceptance.py --binary EXECUTABLE --output RESULT
python3 tools/index-resolution-acceptance.py --verify RESULT
```

The evaluator runs two fresh processes per workload, disables the facts cache and records peak RSS.
Output digests use repository-relative paths. OS caches remain active. The baseline probe and
pre-change implementation live in commit `0150b4cb`; its recorded baseline remains immutable.
