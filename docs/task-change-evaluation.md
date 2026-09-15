# Reviewed task-change evaluation

Roadmap PR 13 added `fr task-change` after task bundles exposed two manual manifest joins. PR 30
extends it into a complete change session: targets may embed bounded fragments, retained full
handles remove repeated project requests, declared checks can run before the first mutation and
successful stage evidence can use a compact outcome shape.

## Controlled comparison

The retained [measurement](../tests/agent-eval/task-change-context.json) uses a generic Rust source,
one body fragment and one declared compiler check. All three arms discover the target and its caller,
replace the same body, enforce four exact postconditions, exercise undo and redo, and emit a patch.

The composed arm counts three manifests and five calls: project task, author preview, author save,
workflow preview and workflow write. Both direct arms count one manifest, one preview and one write.
The change-session arm embeds its fragment, runs the original check before apply and uses compact
success evidence.

| Measure | Composed | Task change | Complete session |
|---|---:|---:|---:|
| Calls | 5 | 2 | 2 |
| Median counted context | 4,623 tokens | 3,740 tokens | 3,712 tokens |
| Median counted context | 13,686 bytes | 11,296 bytes | 10,965 bytes |
| Median local wall time | 0.320 s | 0.315 s | 0.342 s |

All three rotating repetitions produce equal normalized history, final source and patch
identities. Task change also records the selected check requirement on its transaction. The older
composed path verifies the same checks during workflow preflight but does not bind them at planning.
The session has an additional original-check stage, so its stage-list identity intentionally differs.

Token counts replace temporary roots, elapsed milliseconds and opaque hexadecimal identities with
fixed representatives. Byte counts and semantic identities retain original values. The report binds
the optimized binary, measurement sources, tokenizer vocabulary, fixture and every raw run.

```sh
cargo build --release --locked --bin fr
target/agent-eval-venv/bin/python tools/task-change-context.py --fr target/release/fr --repetitions 3 --tokens
python3 tools/task-change-context.py --audit tests/agent-eval/task-change-context.json
```

This prescribed comparison does not measure agent adoption, hidden reasoning, billed quota or
population behavior. Local subprocess timing does not predict hosted latency. A future agent trial
needs explicit authorization for its project payload and should follow a material workflow change.
