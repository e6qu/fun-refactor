# Repeated agent trials on the regex workspace

The next evaluation uses the complete regex workspace at commit `2b527599eb9eea0dcc288c704584f242f26a5c61`.
It contains seven workspace packages, 227 Rust files and 5,553,380 Rust source bytes, without artificial background source.
Preparation and a controlled rehearsal pass. Fresh autonomous trials have not run, so this milestone has no agent context or success-rate result.

## Task and source

Add a documented public `regex::escape_into(pattern: &str, buf: &mut alloc::string::String)` function.
It must append escaped text, preserve existing buffer contents, avoid an intermediate escaped allocation and support builds without default features.
The task requires discovering and using functionality across the regex and regex-syntax package boundary.
Only the facade's `src/lib.rs` needs modification; this does not evaluate coordinated edits in several packages.
The prompt permits reuse of workspace functionality without identifying the helper's location.

The [source archive](../tests/agent-eval/regex/workspace.tar.gz) comes from the [pinned upstream repository](https://github.com/rust-lang/regex/tree/2b527599eb9eea0dcc288c704584f242f26a5c61).
GitHub supplied the [commit archive](https://codeload.github.com/rust-lang/regex/tar.gz/2b527599eb9eea0dcc288c704584f242f26a5c61).
Its SHA-256 is `7f8beace6ed6c94b2eec3c5aa92219e10e73bd8ad696035877bed01d3ee47255`.
The existing cached regex release's Cargo metadata identifies the same upstream commit.
The archive includes upstream's MIT and Apache-2.0 notices; separate copies accompany the fixture.

Source text and manifests remain unchanged at preparation time.
Extraction rejects links and unsafe member paths, and normalizes regular/executable modes to 0644/0755, as in a Git checkout.
The archive's original 0664/0775 modes otherwise differ from files recreated by Git patch application.
This normalization happens before either arm's initial snapshot.

The fixture adds a separately [pinned Cargo.lock](../tests/agent-eval/regex/Cargo.lock), declared checks and disposable Git metadata.
The lock SHA-256 is `7ae225f5fbac82509d5c259dd3613a809c75a8bd4e2124b9cb69ed29d98de3a0`.
Preparation tracks the lock even though upstream ignores it. Checks use `--locked --offline`; manifests retain their actual workspace path dependencies.
The initial dependency fetch resolves the lock once. Later runs use those versions and need no network after fetching them.

## Checks and independent oracle

Every validation stage runs both declared checks together:

- `upstream`: the regex and regex-syntax library test suites, totaling 154 tests in the initial preflight. This excludes integration and documentation tests.
- `minimal`: compile regex with no default features, preserving the public facade's allocation-only build support.

The caller-side oracle uses an explicit metacharacter reference and 3,174 input/prefix combinations, with two appends per combination.
Cases include every metacharacter, ordinary punctuation, empty inputs, whitespace, NUL, Unicode and repeated longer text.
An allocation-counting harness requires no allocation when the destination already has sufficient capacity.
The original workspace fails because the requested facade API is absent; a dependency or unrelated compiler error cannot establish that baseline.
The oracle runs against the changed project and its independent patch receiver.
These finite checks do not prove correctness for every possible string or establish general allocation behavior.

The [controlled rehearsal](../tests/agent-eval/regex/rehearsal.json) uses a prescribed implementation and the real fr binary.
It passes saved-plan application, original/changed/undone/redone checks, exact snapshots, unrelated-edit preservation and patch delivery with unchanged indexes.
Negative controls compile but fail the oracle when they clear the prefix, skip escaping or allocate an intermediate string.
The report identifies the frozen binary, source archive and dependency lock by digest.
This rehearsal supplies infrastructure and behavioral evidence; it is not an autonomous trial or a context measurement.

The rehearsal exposed a product prerequisite: regex denies missing documentation on public APIs.
M4k allows leading `///` and `/** ... */` documentation during Rust function insertion, with the existing size, syntax and history guards.
The signature report excludes documentation, and a separate field identifies its source region.
Other outer attributes and ordinary surrounding comments still refuse. See [function authoring](body-authoring.md#declaration-insertion).

## Reproduction and trial design

Bootstrap dependencies from a disposable copy outside any Cargo workspace:

```sh
python3 tools/regex-workspace-check.py unpack /tmp/fr-regex-deps
CARGO_HOME="$PWD/target/cargo-home" cargo fetch --manifest-path /tmp/fr-regex-deps/Cargo.toml --locked
python3 tools/regex-workspace-check.py check --fr target/debug/fr
```

The controlled check uses its own temporary projects. It does not change the dependency-bootstrap source or run agents.
It needs Cargo, rustc and Git. Normal CI runs the archive, protocol and scoring regressions without building this additional workspace.
The workspace rehearsal is an explicit check after its locked dependency cache is ready.

Freeze the intended binary before preparing four sessions:

```sh
python3 tools/agent-eval.py prepare --project regex --repetitions 2 --out /tmp/fr-regex-agents --fr /path/to/frozen/fr
```

This produces `regex-escape-into-fr-r1`, `regex-escape-into-files-r1`, `regex-escape-into-fr-r2` and `regex-escape-into-files-r2`.
Each session has its own project, clean receiver, skill snapshot, original oracle result and generated prompt.
Use a fresh agent for each prompt, with the same model and effort and no source or solution sharing between sessions.
Report execution order, host contention, refusals and any human intervention. Two repetitions remain a small sample.
Agents must run all declared checks together at each stage; grading refuses a stage that omits either check.
The read/search/edit boundary and token accounting follow the [initial protocol](agent-acceptance.md).

Score every completed session with the pinned tokenizer before recording the complete cohort:

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py score /tmp/fr-regex-agents/regex-escape-into-fr-r1
python3 tools/agent-eval.py record /tmp/fr-regex-agents /tmp/fr-regex-evidence --execution-note 'Describe the actual runtime, isolation and interventions here.'
```

Repeat scoring for the other three sessions. Recording checks the planned cohort and preserves scored failures even when no patch exists.
Runtime provenance comes from the supplied note; the recorder does not assume that fresh agents ran.
Behavioral replay refuses failed trials. Token auditing reads each retained prompt and instrumented transcript.
Default preparation still creates the original four strsim sessions, and both earlier evidence bundles remain immutable.
Sixteen harness regressions and replay of all eight earlier patches pass, alongside the full native/WASM gate and strict kernel verification.
