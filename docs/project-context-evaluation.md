# Initial project-view evaluation

The M2a surface reduces output while preserving selected structural facts.
This measurement compares complete maps with the legacy `symbols --json` response.
It checks every nonlocal symbol's path, name, kind and source line against that response.
It does not measure an agent's ability to finish a code change.

Reproduce from the repository root:

```sh
cargo build --bin fr
python3 tools/project-context.py web/sample
python3 tools/project-context.py src/cli.rs
```

The script disables the fact cache, requests every map page and counts actual stdout bytes.
It records elapsed process time, tool calls, source bytes and the binary's SHA-256 digest.
The complete map uses depth 64 and fields `id,parent,kind,name,path,line`.
Both fixtures fit one 500-row page.
The default 80-row map provides a smaller first inspection with explicit continuation.

| Fixture | Legacy symbols | Nonlocal identities checked | Legacy bytes | Complete map bytes | Reduction |
|---|---:|---:|---:|---:|---:|
| `web/sample` | 501 | 347 | 311,183 | 22,957 | 92.62% |
| `src/cli.rs` | 1,136 | 333 | 711,299 | 16,525 | 97.68% |

The sample's source files containing symbols total 31,639 bytes.
The CLI file totals 202,364 bytes.
This puts the complete maps below those source volumes by 27.44% and 91.83%, respectively.
A source-reading baseline would still need a task-specific selection policy.

The initial local run took 0.30 seconds for sample symbols and 0.31 seconds for its map.
The CLI-file queries took 3.54 seconds and 1.53 seconds.
These single-run timings do not establish a performance guarantee.
Process scheduling, filesystem state and parser startup can affect them.

The identity check covers the existing index's structural facts.
It excludes variables and parameters by design and does not validate inferred architecture.
The small sample includes multiple languages; the CLI file exercises a larger real source file.
Neither substitutes for an unfamiliar-repository task evaluation.

The CLI tests separately cover hierarchy, signatures, bounded source reconstruction, reference confidence,
pagination, coverage gaps, long labels and stale revisions.
The Lean page-length model proves bounds, progress and partition laws.
A corpus of 1,728 cases compares its arithmetic with Rust on 64-bit hosts.
These proofs do not establish parser correctness or agent task success.

The [real-agent acceptance evaluation](agent-acceptance.md) now measures four task outcomes and retrieved context with a pinned reference tokenizer.
Both fr tasks pass, but their retrieved context exceeds the ordinary-file baseline on the small Rust project.
The [context-reduction follow-up](agent-context-followup.md) adds targeted lookup and repeats those tasks with four fresh agents.
Its fr trials retrieve less context than before, while still exceeding the fresh file-tool baseline.
Those task measurements do not replace this script's structural identity checks.
`tools/project-context.py` still reports `model_tokens: null` rather than estimating tokens from byte counts.
Broader evaluation must cover package boundaries, implementation relationships, framework facts and relevant tests on additional projects.

## Bounded source during name lookup

M4o adds `project find NAME --source --bytes N` when a task already needs the selected implementation.
The lookup retains its rows, scope, coverage and revision, and appends a source slice to each returned row.
One raw UTF-8 byte budget is shared across the page in row order, rather than multiplied by the number of matches.
The default budget is 2,048 bytes; the accepted range is 4 through 65,536.
JSON escaping and metadata are outside this source-text budget.

Slices use the same spans, byte offsets and continuation fields as `project show --source`.
A depleted budget leaves later rows visible with empty source text and `next_offset: 0`.
Resume any incomplete slice with `project show HANDLE --source --offset NEXT --bytes N`.
Continue row pages with the same source mode and byte budget; changed options or source revisions refuse stale cursors.
The source option is opt-in. Default lookup reports and cursors keep their existing shape.
Use `show` when node positions, child counts or relationships are also needed.

A [controlled comparison](../tests/agent-eval/find-source-context.json) uses the two functions inspected by the regex workspace trials.
For each prescribed name and file scope, it compares a signature lookup plus a source read against one lookup containing both.
The combined report preserves the complete lookup result and the exact selected source slice.
Tracked source and Git index bytes remain unchanged.

| Selected function | Separate lookup/read tokens | Combined lookup tokens |
|---|---:|---:|
| `regex::escape` | 948 | 556 |
| `regex_syntax::escape_into` | 993 | 598 |

The measured payload reductions are about 40%, with one command instead of two for each selection.
Counts use the agent harness's visible JSON wrapper and pinned tiktoken 0.12.0/o200k_base tokenizer.
Fresh temporary roots change revision and handle strings, so reruns can vary slightly in token counts.
These prescribed queries do not measure discovery, autonomous behavior, total task context or latency.
The separate `show` report includes extra node metadata; callers needing those facts should still request them.
The report retains actual payloads, arguments, binary and fixture digests, and measurement-script hashes.

```sh
python3 tools/find-source-context.py --fr target/debug/fr
target/agent-eval-venv/bin/python tools/find-source-context.py --fr target/debug/fr --tokens
```

This read-only comparison needs the retained source archive, Python and Git, but does not build the regex workspace or require its extra dependencies.
CLI regressions cover shared budgets, empty slices, UTF-8 continuation, empty matches, large bodies and cursor/handle refusal.
The executable authoring example compiles and tests a wrapper after reading its helper directly from the lookup.
M4p adds [source-slice and shared-budget model proofs](lean-specs.md#bounded-source-kernels), with shared Rust cases and actual CLI comparisons.
Parser behavior and general implementation correspondence remain unproved. The retained M4o measurements describe their original binary and inputs.

## Query time and the fact cache

M4q measures the existing cache separately from returned context.
`tools/project-cache.py` runs the same bounded source lookups for `regex::escape` and `regex_syntax::escape_into` on the pinned workspace.
It compares three modes: `--no-cache`, an empty fact cache, and a cache that a preceding identical query populated.
Each mode gets its own temporary `FUN_REFACTOR_CACHE` directory outside the project.
The script rotates mode order across three repetitions and retains every elapsed time, request, output digest and cache inventory summary.

Every measured query must return byte-identical JSON, including source, revisions, handles, coverage and omissions.
Disabled-cache runs must leave their cache directories absent; empty-cache runs must create entries.
Populated runs must preserve their existing cache files and contents. The CLI does not expose per-query hit counters for project commands.
Two additional probes insert a temporary comment inside the selected function.
Cached and uncached queries must return identical changed reports, reject the old handle and restore the original report after source restoration.
The probes must create new content-keyed cache entries. Tracked source bytes, modes and Git index bytes must finish unchanged.

The [retained comparison](../tests/agent-eval/project-cache.json) records the validated debug binary's SHA-256, fixture hashes, runtime and measurement sources.
All eighteen timed queries and both invalidation probes pass.

| Lookup | Cache disabled, median seconds | Empty cache, median seconds | Populated cache, median seconds |
|---|---:|---:|---:|
| `regex::escape` | 10.088 | 10.233 | 3.280 |
| `regex_syntax::escape_into` | 10.203 | 10.219 | 3.282 |

Populated-cache medians are 67.5% and 67.8% below the disabled-cache medians on this host and binary.
Empty-cache medians remain near disabled-cache medians; populating the cache adds no demonstrated first-query benefit here.
Each populated inventory starts with 244 files and roughly four megabytes of cached data.
The source probes create additional entries and restore the original query results without clearing old entries.
An initial local run showed the same direction; the retained report contains the separate confirmation run and all of its samples.

Its summaries exclude priming queries, while retaining their elapsed times separately.
Inventory hashing runs outside each timed query and reads the populated cache entries.
The script creates empty fr caches; it does not flush operating-system caches or establish cold-disk behavior.
Subprocess wall times include startup, scanning, indexing, project construction, source verification and JSON output.
These measurements do not profile individual phases, run agents, build regex or establish production latency.
Identical responses mean this cache comparison changes no returned-context size. Existing autonomous trial results remain historical evidence.

```sh
python3 tools/project-cache.py --fr target/debug/fr --repetitions 3
```

The script needs the retained regex source archive, Python and Git. It uses no extra Cargo dependencies or tokenizer.
Cache failures and invalidation assertions fail the measurement; elapsed time has no pass/fail threshold.
Three evaluator regressions reject changed report fields and incomplete source, and check restoration after an injected query failure.
Before another autonomous comparison, state the cache policy and distinguish task context from query latency.
The release profile below narrows the remaining work before any daemon or persistent project-index decision.

## Release stage profiling

M4r repeats the cache comparison with an optimized CLI and adds a separate library-stage profiler.
Build both executables from the same checkout and locked dependency set:

```sh
CARGO_HOME="$PWD/target/cargo-home" CARGO_NET_OFFLINE=true cargo build --release --locked --bin fr --example project-profile
python3 tools/project-cache.py --fr target/release/fr
python3 tools/project-profile.py --fr target/release/fr --profiler target/release/examples/project-profile
```

The [ordinary release CLI report](../tests/agent-eval/project-cache-release.json) retains all eighteen samples and both passing source-invalidation probes.

| Lookup | Cache disabled, median seconds | Empty cache, median seconds | Populated cache, median seconds |
|---|---:|---:|---:|
| `regex::escape` | 1.438 | 1.431 | 0.180 |
| `regex_syntax::escape_into` | 1.419 | 1.420 | 0.180 |

The [stage report](../tests/agent-eval/project-profile.json) records eighteen separate samples from `tools/project-profile.rs`.
This Cargo example calls the public library pipeline with default scan options and an explicit project directory.
Every result must match the ordinary CLI's complete JSON bytes on the same fixture.
All populated samples record 249 fact-cache hits for 249 indexed files. These counters exclude resolution-cache hits.
Each sample starts a new process; cache directories, rotating order and inventory checks follow the M4q method.

| Populated-cache stage | `escape`, median ms | `escape_into`, median ms |
|---|---:|---:|
| Initial scan | 3.109 | 3.232 |
| Index construction | 10.623 | 10.459 |
| Project construction | 145.716 | 145.725 |
| Selected query | 0.878 | 0.858 |
| Final source/inventory verification | 8.905 | 8.759 |
| Cleanup | 4.606 | 4.569 |

Root resolution, cache opening and JSON serialization each take under one millisecond in these medians.
The full profiled subprocess medians are 179.2 and 178.9 ms; medians of individual phases need not sum to a median total.
Stage timers exclude argument parsing, process startup and the profiling envelope's output. The subprocess timer includes them.
All phases must be nonnegative and fit within the internal measured interval, which must fit within subprocess time.
The evaluator regression rejects absent phases, negative intervals and overlapping or oversized totals.

Project construction accounts for roughly four-fifths of the populated-cache subprocess time in these samples.
That phase captures manifests and source, builds line indexes and hierarchy, and constructs the revision digest.
M4s separates those costs and optimizes revision hashing, as described below.
Index time includes fact loading or extraction, merging and workspace resolution; this profiler does not separate those operations.

The release measurements concern this binary, two prescribed queries and one host.
They do not change the earlier debug measurements or autonomous results, establish general production latency, or demonstrate context savings.
M4r introduced no optimization or daemon and left ordinary CLI reports and production library code unchanged.

## Batched revision hashing

M4s adds optional construction checkpoints to the development profiler and batches the revision digest's serialized inputs.
`Project::new_profiled` records manifest capture, setup, source reads, source hashing, line indexes, hierarchy, symbol hashing, reference hashing and finalization.
The ordinary constructor disables these checkpoints at compile time. Ordinary CLI reports contain no timing fields.
Source rereads, content checks and final source/inventory verification remain in place.

Revision construction serializes the same values in the same order and hashes the same concatenated JSON bytes with SHA-256.
A reusable buffer flushes after a complete item brings it to at least 65,536 bytes, and again at finalization.
This is a flush threshold, not a capacity limit: one item can exceed it, and the buffer retains its largest allocation until construction ends.
Failed serialization discards only the partially appended item before returning its error.
Tests cover explicit JSON byte sequences, Unicode and escapes, large items, threshold boundaries and partial-write failures.
These tests establish correspondence on their cases; they do not formally prove the serializer, SHA-256 implementation or complete revision construction.

The controlled comparison alternates old and new release executables across four repetitions of each prescribed lookup.
Each side has a separate populated cache; all profiled samples require 249 fact-cache hits for 249 indexed files.
Every CLI and profiler result must match the baseline's complete JSON, including source, revisions and cursors.
Additional comparisons traverse every package, dependency, workspace, gap and default nonlocal map page: 22 pages on this fixture.
Two source-invalidation probes require cached/uncached agreement, stale-handle refusal and exact restoration.
Tracked source bytes, file modes and Git index bytes finish unchanged.

The [retained comparison](../tests/agent-eval/project-construction.json) contains sixteen ordinary CLI samples and sixteen separate development profiles.
All samples and both invalidation probes pass.

| Lookup | Baseline CLI, median ms | Batched CLI, median ms | Baseline construction, median ms | Batched construction, median ms |
|---|---:|---:|---:|---:|
| `regex::escape` | 179.481 | 168.476 | 147.640 | 134.827 |
| `regex_syntax::escape_into` | 182.837 | 172.008 | 148.068 | 135.166 |

Ordinary CLI medians improve by 6.1% and 5.9% in this run.
Reference serialization and hashing remain the largest construction stage, falling from about 110 ms to 98 ms.
The other stages together account for roughly 37 ms after batching.

Construction checkpoints include timing overhead. Batching can charge earlier items' hash work to the stage that flushes their bytes.
Use ordinary CLI samples for end-to-end comparison and nested phases to locate costs, rather than treating each stage as isolated work.
Independent phase medians need not sum to median construction time.
This measures two known lookups on one host, with no cold-start, autonomous context-saving or general production-latency claim.
Earlier measurement artifacts remain historical and unchanged.

To reproduce, first build the baseline CLI at commit `9a253e9` in a disposable checkout and save the executable outside its target directory.
Apply the retained [instrumentation-only patch](../tests/agent-eval/project-construction-before.patch) there and build the baseline profiling example.
The patch preserves the baseline's five per-item revision digest updates; only timing and the example's envelope change.
Save that profiler separately, then build both candidate executables from the current checkout:

```sh
CARGO_HOME="$PWD/target/cargo-home" CARGO_NET_OFFLINE=true cargo build --release --locked --bin fr --example project-profile
python3 tools/project-construction.py --before /path/to/baseline-fr --before-profiler /path/to/baseline-project-profile
```

Both baseline builds use the same locked dependencies. The comparison needs Python, Git and the pinned archive, without building regex or loading a tokenizer.
The report records all samples, executable digests, measurement-source hashes and the instrumentation patch hash.
Timing has no pass/fail threshold; report differences, invalid phase intervals, cache misses and failed restoration reject the measurement.
The full native/WASM gate passes, including all 131 project CLI scenarios and 311/311 capability coverage.
Strict verification retains twenty-three fresh source anchors and signature maps, zero obligations and 31 Lean build jobs.
M4s added no new Lean proof; its digest correspondence rested on the explicit byte tests and controlled report comparisons above.
M4t adds [revision buffer model proofs](lean-specs.md#revision-buffer-kernels) and shared state comparisons without replacing this timing evidence.
