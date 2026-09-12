# Agent semantic IR SDK evaluation

This evaluation asks which authoring surface helps a fresh agent construct exact semantic IR. It
compares direct JSON with the Python SDK on one generic program pattern.

The task constructs a source-free body with three steps. A mutable integer total starts at zero. A
loop adds each positive item from `values`, then returns the total. The expected value uses one type,
seven statement forms, and five expression forms.

`tools/agent-ir-sdk-trial.py` prepares and scores the pair. Each arm receives a copied `fr` binary
and the same expected canonical value. The SDK arm also receives the installed `fr_ir` package. The
scorer requires successful Rust validation, exact canonical equality, the expected producer form,
and zero SDK implementation-source reads.

Both fresh sessions used Codex CLI 0.154.0 with `gpt-5.6-luna`, low reasoning, and the default
service tier. They ran sequentially with ephemeral state, ignored user configuration and rules, and
used the workspace-write sandbox. No human corrected or restarted either scored run.

| Measure | Python SDK | Direct JSON |
|---|---:|---:|
| Exact canonical result | yes | yes |
| Implementation-source reads | 0 | 0 |
| Shell commands | 13 | 4 |
| Input tokens | 199,405 | 109,963 |
| Cached input tokens | 172,032 | 93,440 |
| Input tokens excluding reported cache hits | 27,373 | 16,523 |
| Output tokens | 1,806 | 1,812 |
| Reasoning output tokens | 303 | 133 |
| Producer bytes | 701 | 1,445 |
| Elapsed seconds | 54.3 | 45.7 |

The direct route used 65.7% fewer input tokens after subtracting reported cache hits and nine fewer
commands. The SDK producer used 51.5% fewer bytes. Output tokens were effectively equal. This one
trial favors direct JSON for a small one-off body whose complete shape fits in bounded catalog
pages. The SDK gives earlier category checks and a shorter reusable artifact, but it did not reduce
agent context here.

An earlier diagnostic pair made the exact change, but its SDK agent opened `fr_ir/__init__.py` four
times. That exposed two defects. Category pages omitted Python constructor names, and the scorer did
not enforce the no-source rule. The implementation now publishes constructors beside every shape,
and scoring fails any implementation-source read. The scored pair used that revision.

The result does not establish a population effect. It uses one model, one task, one repetition, and
one execution order. The prompts differ by 41 bytes because they prescribe different producer
forms. The task tests construction and validation without selecting or editing a real project.
Future cohorts should rotate order and include repeated edits where SDK reuse can repay discovery.

The retained evidence lives under `tests/agent-eval/results/2026-09-11-semantic-ir-sdk`. Its manifest
binds every prompt, event stream, run record, result, expected value, and produced artifact. The
smaller deterministic comparison remains in `tests/agent-eval/semantic-ir-sdk.json`.
