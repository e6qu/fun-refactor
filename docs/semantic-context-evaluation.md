# Semantic context evaluation

The retained comparison exercises one generic Rust function-body change through the source-fragment
and semantic-body routes. Both arms inspect the target, write it, undo, redo and generate a Git
patch. They produce identical final source and patch bytes while preserving an unrelated function.

| Route | Calls | Visible output bytes | Request bytes | Input bytes | Source text exposed |
|---|---:|---:|---:|---:|---:|
| Source fragment | 5 | 4,222 | 357 | 26 | 48 |
| Semantic body | 5 | 4,391 | 389 | 179 | 0 |

The semantic report contains seven complete tagged nodes and costs 954 bytes in `--minimal` mode.
The complete semantic route exposes no source text. It costs 169 more visible output bytes and 153
more input bytes on this small change. This result establishes the source boundary and an exact cost
baseline; it does not establish a context reduction or an agent success rate.

Direct `--declaration` selection keeps both arms at five calls. Compact default-field serialization
and `--minimal` reduce the semantic report while retaining its semantic basis, selected handle,
budget, omissions, source policy, model and patterns. `report_omitted` records which generic project
fields it omitted.

The executable comparison checks source and patch identity, source exposure, semantic completeness,
writer fidelity and apply/undo/redo. Reproduce the retained report with:

```sh
python3 tools/semantic-context.py --fr target/debug/fr --output tests/agent-eval/semantic-context.json
```

These byte counts use compact CLI JSON on one synthetic function. They exclude prompts, skill reads,
hidden reasoning and tokenizer behavior. A fresh economical-agent trial remains useful after agents
have a task agents can complete from the semantic model without requesting source.
