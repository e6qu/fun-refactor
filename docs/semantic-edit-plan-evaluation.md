# Semantic edit-plan evaluation

This deterministic comparison applies the same exact integer edit through the explicit
`fr-semantic-intent-1` route and direct scalar planning. Both routes operate on a generic Rust
arithmetic function and expose no source through `fr` reports.

`tools/semantic-edit-plan-eval.py` runs the real CLI in isolated workspaces. Each route previews and
writes the change under its complete plan basis, compiles and runs the changed test, checks forward
and reverse patches, restores exact original bytes through undo, reapplies exact changed bytes
through redo, and reads the final source-free semantic identity.

| Measure | Explicit intent | Scalar plan |
|---|---:|---:|
| Commands before lifecycle checks | 3 | 2 |
| Query output | 1,478 bytes | 0 bytes |
| Authored payload | 425 bytes | 0 bytes |
| Preview output | 2,870 bytes | 3,627 bytes |
| Compacted write output | 614 bytes | 617 bytes |
| Counted authoring context | 5,387 bytes | 4,244 bytes |
| Behavior, patch, undo and redo | pass | pass |

The direct route resolves the exact declaration below `.` during author preview and generates the
intent internally. It removes one command and 1,143 counted bytes, a 21.2% reduction in this fixed
sequence. Its preview is larger because it carries the complete generated intent as review
evidence. Both routes have equal canonical `fri1:` intent, compiled-change, final semantic-body and
source identities. Their raw input SHA-256 values differ because one input is caller-serialized and
the other uses the tool's canonical serialization.

The retained report is `tests/agent-eval/semantic-edit-plan.json`. It measures UTF-8 payload and CLI
output bytes for the two authoring sequences. It excludes skill reading, model tokens, cache
behavior, compiler output and common lifecycle reports. This is a controlled single-fixture result,
not evidence of population-level agent behavior.

Reproduce it with:

```sh
python3 tools/semantic-edit-plan-eval.py --fr target/debug/fr \
  --output tests/agent-eval/semantic-edit-plan.json
```
