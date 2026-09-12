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

Reproduce it with:

```sh
python3 tools/semantic-delta-eval.py --fr target/debug/fr \
  --output tests/agent-eval/semantic-delta.json
```
