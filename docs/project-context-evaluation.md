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
