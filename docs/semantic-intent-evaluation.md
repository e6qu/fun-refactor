# Semantic intent evaluation

This deterministic evaluation compares complete-body replacement, pointer delta, direct semantic
intent and Python semantic intent on one generic six-statement Rust arithmetic pipeline. Every route
changes the same integer scalar from `1` to `7`.

`tools/semantic-intent-eval.py` creates four isolated workspaces and runs the real CLI. Each route
must compile and pass the changed test, produce the same canonical body and source identities,
check forward and reverse patches, and restore and reapply exact bytes through undo and redo.

| Measure | Complete body | Pointer delta | Direct intent | Python intent |
|---|---:|---:|---:|---:|
| Semantic query | 2,066 bytes | 3,250 bytes | 1,479 bytes | 1,479 bytes |
| Payload | 1,081 bytes | 245 bytes | 425 bytes | 361 bytes |
| Python producer | 0 bytes | 0 bytes | 0 bytes | 535 bytes |
| Preview | 2,338 bytes | 3,038 bytes | 2,914 bytes | 2,914 bytes |
| Write | 2,435 bytes | 3,135 bytes | 3,011 bytes | 3,011 bytes |
| Measured total | 8,609 bytes | 10,357 bytes | 8,518 bytes | 8,989 bytes |
| Behavior, patch, undo and redo | pass | pass | pass | pass |

The filtered locator-only query plus direct intent uses 39.5% fewer query-and-payload bytes than the
complete-body route. Its complete measured total is 1.1% smaller. It uses 17.8% fewer total bytes
than the pointer route because the query omits both the semantic body and unrelated locator rows.
The Python producer makes that route 5.5% larger than direct intent in this one-off case.

The retained report is `tests/agent-eval/semantic-intent.json`. It measures UTF-8 process output,
payloads and the Python producer. It does not measure model tokens, cache behavior, agent success or
population effects. The filtered query assumes the operation and exact current scalar are known;
broader discovery returns more rows. A fresh economical-agent trial remains separate evidence.

Reproduce it with:

```sh
python3 tools/semantic-intent-eval.py --fr target/debug/fr \
  --output tests/agent-eval/semantic-intent.json
```
