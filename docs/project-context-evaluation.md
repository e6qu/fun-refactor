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
Source slicing reuses the modeled page-length helper; these additions make no new proof claim about parsing or aggregate query behavior.
