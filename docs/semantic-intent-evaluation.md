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
| Preview | 2,295 bytes | 2,995 bytes | 2,951 bytes | 2,951 bytes |
| Write | 2,392 bytes | 3,092 bytes | 3,048 bytes | 3,048 bytes |
| Measured total | 8,523 bytes | 10,271 bytes | 8,592 bytes | 9,063 bytes |
| Behavior, patch, undo and redo | pass | pass | pass | pass |

The filtered locator-only query plus direct intent uses 39.5% fewer query-and-payload bytes than the
complete-body route. Its complete measured total is 0.8% larger after adding raw and canonical
intent identities to the review report. It uses 16.2% fewer total bytes than the pointer route. The
Python producer makes that route 5.4% larger than direct intent in this one-off case.

The retained report is `tests/agent-eval/semantic-intent.json`. It measures UTF-8 process output,
payloads and the Python producer. It does not measure model tokens, cache behavior, agent success or
population effects. The filtered query assumes the operation and exact current scalar are known;
broader discovery returns more rows.

## Fresh agent pair

One fresh sequential pair used Codex CLI 0.154.0, `gpt-5.6-luna`, low reasoning and the default
service tier. Both ephemeral sessions ignored user configuration and rules. Neither agent read
`app.rs`; both produced the exact expected semantic body and passed the compiled behavior check.

| Measure | Semantic intent | Complete body |
|---|---:|---:|
| Authoring payload | 470 bytes | 1,930 bytes |
| Commands | 7 | 9 |
| Input tokens | 132,199 | 171,450 |
| Cached input tokens | 105,216 | 148,736 |
| Input tokens excluding reported cache hits | 26,983 | 22,714 |
| Output tokens | 1,364 | 2,020 |
| Reasoning output tokens | 312 | 326 |
| Elapsed time | 39.9 seconds | 54.1 seconds |
| Exact body and behavior | pass | pass |

The intent arm used 75.6% fewer payload bytes, two fewer commands, 22.9% fewer total input tokens
and 32.5% fewer output tokens. Its input excluding reported cache hits was 18.8% higher. The trace
made one unsuccessful broad project query before using the exact filtered locator route, so the
result identifies another skill and command-discovery opportunity. One pair does not estimate
population success, stable latency or billing. Cache accounting comes from the CLI event stream.

The complete retained evidence, prompts, command events and scored results are under
`tests/agent-eval/results/2026-09-12-semantic-intent`. A manifest binds every file by SHA-256, and a
Rust test checks the manifest, model, reasoning effort, route use, source-read rule, exact result and
behavior result.

Reproduce it with:

```sh
python3 tools/semantic-intent-eval.py --fr target/debug/fr \
  --output tests/agent-eval/semantic-intent.json
```

Prepare and score a new fresh pair with:

```sh
python3 tools/agent-semantic-intent-trial.py prepare --out /tmp/fr-semantic-intent \
  --fr target/debug/fr
python3 tools/agent-eval-codex.py /tmp/fr-semantic-intent \
  --trial semantic-intent-fr --trial semantic-intent-files --model gpt-5.6-luna \
  --effort low --service-tier default --timeout 900 --confirm-agent-spend
python3 tools/agent-semantic-intent-trial.py score /tmp/fr-semantic-intent
```
