# Reviewed task-change evaluation

Roadmap PR 13 adds `fr task-change` after task bundles exposed two remaining manual manifest joins.
The command resolves the task, validates concrete authoring fragments and runs checked delivery
under one review basis.

## Controlled comparison

The retained [measurement](../tests/agent-eval/task-change-context.json) uses a generic Rust source,
one body fragment and one declared compiler check. Both arms discover the target and its caller,
replace the same body, enforce four exact postconditions, exercise undo and redo, and emit a patch.

The composed arm counts three manifests and five calls: project task, author preview, author save,
workflow preview and workflow write. The task-change arm counts one manifest, one preview and one
write. Both count the common fragment.

| Measure | Composed | Task change | Difference |
|---|---:|---:|---:|
| Calls | 5 | 2 | 3 fewer |
| Median counted context | 4,623 tokens | 3,736 tokens | 887 tokens (19.2%) fewer |
| Median counted context | 13,676 bytes | 11,274 bytes | 2,402 bytes (17.6%) fewer |
| Median local wall time | 0.393 s | 0.361 s | 8.1% lower |

All three rotating repetitions produce equal normalized stage, history, final source and patch
identities. Task change also records the selected check requirement on its transaction. The older
composed path verifies the same checks during workflow preflight but does not bind them at planning.

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
