# Verified change workflow evaluation

Roadmap PR 10 adds `fr workflow` after retained agent traces showed high change-and-delivery cost.
The command carries one reviewed transaction through apply, declared checks, undo, restored checks,
redo, reapplied checks and optional patch creation.

## Controlled comparison

The retained [measurement](../tests/agent-eval/workflow-context.json) uses a generic Python source
file and one declared syntax check. Preparation, the change preview, plan persistence, transaction
inspection and check listing are common setup. The comparison begins after the agent has reviewed
those inputs.

The manual arm makes seven calls. It applies, checks, undoes, checks, redoes, checks and exports a
patch. The workflow arm counts its 318-byte manifest, one preview and one write with the preview's
`frwb1:` basis. Both arms use compact transition results and quiet checks without repeated declarations.

| Measure | Manual | Workflow | Difference |
|---|---:|---:|---:|
| Calls | 7 | 2 | 5 fewer |
| Median counted context | 2,047 tokens | 1,880 tokens | 167 tokens (8.2%) fewer |
| Median counted context | 6,368 bytes | 5,556 bytes | 812 bytes (12.8%) fewer |
| Median local wall time | 0.284 s | 0.262 s | 7.7% lower |

All three rotating repetitions produce the same normalized stage identity in both arms. Final
history state, check receipts, source bytes and patch bytes also match. The optimized release binary,
measurement sources, tokenizer vocabulary, fixture and every raw run remain checksum-bound.

Token counts replace temporary roots, elapsed milliseconds and opaque hexadecimal identities with
fixed representatives. Byte counts and semantic identities retain original values. The audit
recomputes every measurement-source digest, paired identity and summary statistic.

```sh
cargo build --release --locked --bin fr
target/agent-eval-venv/bin/python tools/workflow-context.py --fr target/release/fr --repetitions 3 --tokens
python3 tools/workflow-context.py --audit tests/agent-eval/workflow-context.json
```

The measurement prescribes the post-review commands. It does not measure skill loading, planning,
agent adoption, task success, independent behavior or receiver checks. Local subprocess timing does
not predict hosted latency. A fresh trial must establish whether an agent adopts the shorter route.
