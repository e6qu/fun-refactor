# Contribute a change and check it

Use this guide when you need to change `fr`, add a language reader or prepare a release.
Start with the [focused roadmap](../PLAN.md#next-work): selected smaller agents, specific refactorings and repairs, and Lean proof writing.
Tie new language or analysis support to a named task and its independent check before extending the implementation.
For example, a parser fix needs both a case that now works and a case that still refuses an unsafe edit. Read the workstation limits before running any local command.

A fixture is a small input used by a test.
Source provenance records where copied code came from, including its revision and license.
For a Python import fix, add a small example that distinguishes the old mistake from the intended behavior.
Run bounded local checks through the guard below, and send builds and complete test suites to GitHub.

## Build and contribution instructions

`fr` requires stable Rust, Python 3 and the toolchains used by the language fixtures. Lean tests
use the repository's pinned Lake project. Zig and Go tests keep their caches under `target` by
default. For the shared workstation, read the resource rules below before running commands.

## Working on a shared desktop

Full builds, complete test suites, study tasks and large report regeneration run on GitHub.
Local commands, including `fr` and lightweight checks, run serially through:

```sh
python3 /Users/zardoz/.codex/tools/fr-local-guard.py COMMAND...
```

The guard lowers priority, throttles sampled CPU toward half of one core and enforces a
180-second deadline. It samples the combined resident memory of the command and its child processes.
It stops at 1 GiB of that memory, 2 GiB of generated `target` data,
or less than 64 GiB free disk. If it refuses, move the workload to CI; do not bypass or raise limits.
One compiler worker alone does not control memory or disk growth. Sampling can miss brief peaks.

Preserve source and the working executable during cache cleanup. Rebuild only when needed, on
a suitable runner. Use configured OpenCode access and the existing `gh` login; do not inspect,
copy or upload credential files. Studies also have agreed task, cost and stop rules; meet those before starting a trial.

## Build and test on a dedicated runner

Build the native CLI with the locked dependency graph:

```sh
cargo build --locked
cargo run --locked --features cli -- --version
```

The check script covers the native, Python and WASM suites. These run on pull requests.
CI also audits the original package and project-change records in separate required jobs.
The `deep` suite checks whole-repository reports, every Lean case and patches applied in separate receiver checkouts.
GitHub divides the command-agreement tests into six jobs. It runs the other report checks and
receiver tests separately. Each job has a 15-minute limit; runner queue time can make the
whole workflow take longer. The final job requires every discovered audit test exactly once,
matching source revisions, and successful external replays. `check.sh deep` retains the full
serial suite for machines with sufficient resources.

```sh
PATH="$PWD/sdk/python/.venv/bin:$PATH" tools/check.sh default
tools/check.sh wasm
tools/check.sh deep
```

Rust test fan-out in the default, deep and Lean-kernel gates defaults to one. Lean processes use the
same worker bound. Keep those defaults unless the runner configuration explicitly specifies more
capacity. They do not authorize a local full gate.

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

Use tests for the affected behavior while developing, then run the complete affected suite before merging. The
portable agent skill has an executable example checker:

```sh
cargo test --test agent_skill
python3 tools/check-agent-skill.py --fr target/debug/fr
```

## Review documentation changes on GitHub

Rewording a guide can also change the reading-size measurements that use it. For example, a
shorter agent instruction changes the text returned in an old recorded task, even though that
task's calls and outcome stay the same. Refresh the derived measurement before the final PR checks.

Run the hosted documentation review on your working branch:

```sh
gh workflow run refinement-evidence.yml --ref YOUR_BRANCH \
  -f group=docs-review -f prefix=YYYY-MM-DD-docs-review
```

Replace `YOUR_BRANCH` and the date. This checks guide links, headings, writing and reading limits,
regenerates current report pages and reading-size measurements, and runs their regression tests.
It commits checked generated files back to that branch. Keep the original trial records unchanged.
Pick up that commit before preparing the final change and requesting full CI.

The review covers current manuals, agent references, SDK instructions and our build and provenance notes.
Historical experiment outputs and source fixtures retain their original contents.
For a browser parser build failure, see the [bundled parser build notes](../crates/tree-sitter-language/README.md).

Some guides are also recorded inputs to tests. For example, changing `docs/python-contract.md`
changes the Python report's input fingerprint even when the edit only changes wording.
Finish those edits before collecting fresh results. Use the hosted collectors to refresh affected
reports; preserve their old records. Never change a recorded hash to make stale evidence pass.

## Refresh retained evidence

Fetch current `main` before collecting reports and check it again before publication.
Integrate its changes before fixing the source revision for a collection.
For example, a release can change the Python library's version file while a measurement is running.
If a report records that file, refresh it against the integrated release. Keep the earlier measurement and its original audit available.

`tools/refresh-refinement-evidence.py` refuses local execution and regenerates reports tied to recorded source inputs
on GitHub runners. The [CI guide](ci.md) owns refresh groups and coverage requirements.
The manual `Refresh refinement evidence` workflow uploads results from a directory unique to each
run and attempt under the runner's temporary directory. Reports stay outside the compiler cache.
Supply a new dated `prefix` to retain new report directories.
The default `all` group checks virtual workspace boundaries before measuring seven evidence groups.
The `flow` group runs scalar, package and SDK boundary tests, then refreshes ten flow reports and index measurements.
The `dependency` group refreshes reports for compilers, model-to-code connections, source descriptions,
saved indexes, proofs and recovery after shared dependency changes. The `contexts` group refreshes four workflow comparisons
with the pinned tokenizer. Different groups can run independently on GitHub.
Index-resolution evidence binds every Rust source file and must be refreshed after any Rust source change.
Choose one group to refresh only its measurements. Update audit-test report paths before refreshing:
host-recovery evidence includes its audit test's source identity.
Retain downloaded evidence before requesting the final CI run; never substitute rewritten hashes
for fresh measurements.

## Repeated consumer queries

`Index::references_to` retains extraction order and expands the current definition group.
`reference_count` and `has_references` answer counts and presence without collecting occurrence records.
The first target lookup builds an in-memory reverse index; later queries reuse it.
Name matching builds a separate lookup only when needed. These caches add memory proportional to the reference count.
They do not persist to disk or change resolution confidence, missing consumers or analysis coverage.

`Index::references` uses `index::References`, which dereferences to the reference vector.
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

The original repository count includes six false variable uses in Lean patterns such as
`rcases value with _ | other`. The underscore discards a value; it does not read a variable.
The current reader removes those six uses. The report retains their exact source locations
and checks that they are absent from the index. All other count differences still fail.
The generated Python workloads retain their original counts and answers.
Every measured query must also agree with the separate linear lookup on the current index.

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

Release Please prepares the version and changelog pull request. Merging it publishes the release tag;
the workflow then builds native archives, the WASM package and the Python package, with checksums.
It checks out the commit resolved from the published tag. Preparing the next release pull request runs separately,
so an API error in that bookkeeping cannot block downloads for an already published release.
Existing downloads are never overwritten. Keep installation instructions independent of a specific version number.

If publication stopped after creating the latest release but before uploading any files, explicitly select that tag:

```sh
gh workflow run release.yml --ref APPROVED_WORKFLOW_BRANCH --field existing_tag=fun-refactor-vVERSION
```

Replace the branch with the reviewed workflow branch and `VERSION` with the published release's version.
This completes an existing release; it does not create a tag or release another commit.
The workflow verifies the published tag, its package version and the absence of downloads before building.
If an individual build fails after other downloads have arrived, rerun that failed job in the same run.

Before release, require passing default, WASM and deep lanes and archive-shape checks on GitHub.

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
