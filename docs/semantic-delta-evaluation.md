# Semantic delta evaluation

This evaluation compares one checked semantic delta with complete semantic-body replacement. Both
routes change one integer expression in a generic six-statement Rust arithmetic pipeline. They use
the same source-free semantic query, semantic writer, body splice and source-history lifecycle.

`tools/semantic-delta-eval.py` creates isolated fixtures and runs the real CLI. It checks compiled
behavior, forward and reverse patch bases, exact undo and redo, final body identity and final source
identity. A separate fixture confirms that a stale body basis refuses before writing.

| Measure | Semantic delta | Complete body |
|---|---:|---:|
| Change payload | 245 bytes | 1,081 bytes |
| Semantic query | 2,066 bytes | 2,066 bytes |
| Preview report | 3,038 bytes | 2,338 bytes |
| Write report | 3,135 bytes | 2,435 bytes |
| Behavior, patch, undo and redo | pass | pass |

The delta reduces the change payload by 77.3%. Its operation receipt makes the preview 29.9%
larger and the write report 28.7% larger. An agent that retains the initial semantic query saves
836 authored bytes but receives 700 more result bytes from either authoring call. Compact delta
receipts remain a possible optimization.

The retained report is `tests/agent-eval/semantic-delta.json`. Its scope is one deterministic Rust
fixture. It measures UTF-8 bytes, not model tokens, agent success rates, language coverage or a
population effect.

## Fresh agent pair

One fresh sequential pair used Codex CLI 0.154.0 with `gpt-5.6-luna`, low reasoning and the default
service tier. Both agents read the same semantic skill page, used only source-free project data,
applied their required route, produced the exact semantic body and passed the compiled behavior
check. The retained command trace contains no observed direct source read.

| Measure | Semantic delta | Complete body |
|---|---:|---:|
| Exact result and behavior | pass | pass |
| Direct source reads | 0 | 0 |
| Payload artifact | 331 bytes | 1,930 bytes |
| Commands | 9 | 5 |
| Input tokens | 206,382 | 93,410 |
| Cached input tokens | 174,848 | 76,288 |
| Input tokens excluding reported cache hits | 31,534 | 17,122 |
| Output tokens | 2,101 | 1,584 |
| Reasoning output tokens | 574 | 163 |
| Elapsed seconds | 60.7 | 40.0 |

The delta artifact is 82.8% smaller, while this delta agent used 84.2% more input after reported
cache hits and four more commands. It first sent the delta to the whole-body validator, then needed
three failed previews to find the exact nested pointer. That trace led to two follow-up changes: the
skill now separates body and delta validation, and `project semantic --body --pointers` reports
exact authorable paths, categories and kinds.
On the same fixture, its compact 22-row index adds 1,184 bytes to the semantic query and includes
the exact changed node.

This pair establishes successful end-to-end use under its recorded conditions. One task and one
execution order do not establish a general agent-quality, latency or quota effect. The follow-up
pointer route has deterministic tests but no second agent run.

The retained evidence lives under `tests/agent-eval/results/2026-09-12-semantic-delta`. Its manifest
binds prompts, event streams, run records, results, expected values, semantic skill snapshots and
produced payloads by SHA-256.

Reproduce it with:

```sh
python3 tools/semantic-delta-eval.py --fr target/debug/fr \
  --output tests/agent-eval/semantic-delta.json
```
