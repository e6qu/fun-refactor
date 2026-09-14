# Local Codex agent evaluation

`tools/agent-eval-codex.py` launches prepared paired trials through `codex exec`. It is opt-in, never runs from the normal CI gate and requires an explicit quota acknowledgement.

Prepare a fresh pair, inspect the generated prompts, then dry-run the exact commands. Preparation
copies the selected executable once to `<sessions>/fr-agent-eval-bin`, makes that copy read-only and
binds every arm to its digest. Later Cargo builds cannot silently invalidate a running cohort.

```sh
python3 tools/agent-eval.py prepare --project regex-coordinated --repetitions 1 --out /tmp/fr-agent-sessions
python3 tools/agent-eval-codex.py /tmp/fr-agent-sessions --trial regex-escape-len-fr --trial regex-escape-len-files --dry-run
```

Run the reviewed pair locally:

```sh
python3 tools/agent-eval-codex.py /tmp/fr-agent-sessions --trial regex-escape-len-fr --trial regex-escape-len-files --confirm-agent-spend
```

Each trial is a new `codex exec --ephemeral` invocation. The runner passes `--ignore-user-config`,
`--ignore-rules` and `--skip-git-repo-check` for prepared snapshots. It pins `gpt-5.6-luna`,
`model_reasoning_effort="low"`, `service_tier="default"` and the workspace-write sandbox, then reads
the frozen prompt over standard input. It accepts complete `fr`/`files` pairs by default. An
experiment can declare another ordered pair in `pair_arms`, such as `direct`/`explicit`. The runner
refuses incomplete pairs and sessions with existing run or harness events.

The session retains `codex-events.jsonl`, `codex-stderr.txt`, `codex-final.txt` and `codex-run.json`. The run record binds the prompt and streams by SHA-256 and records model, effort, service tier, elapsed time and exit status. `agent-eval.py record` copies these files when present and includes them in the evidence manifest.

Score both arms before recording them. Bind the manifest to the commit used to build the evaluated binary when recording occurs from a later commit:

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py score /tmp/fr-agent-sessions/regex-escape-len-fr
target/agent-eval-venv/bin/python tools/agent-eval.py score /tmp/fr-agent-sessions/regex-escape-len-files
target/agent-eval-venv/bin/python tools/agent-eval.py record /tmp/fr-agent-sessions tests/agent-eval/results/DATE-context-v2 --implementation-commit COMMIT --execution-note 'Exact execution conditions'
```

The manifest records overall acceptance and failed trial names. Failed cohorts remain recordable as diagnostic evidence and token-auditable, while replay refuses them rather than treating their patches as accepted results.

The [2026-09-14 diagnostic pair](../tests/agent-eval/results/2026-09-14-deferred-diagnostic/manifest.json)
found two evaluator defects. The portable workflow reference described the integer input schema
without showing its exact JSON, and prepared sessions pointed to a mutable Cargo output. The file
arm passed its changed-state oracle before that executable changed. The corrected harness copies one
read-only binary for the pair, fingerprints the evaluator at preparation, and reports every invalid
workflow field with the expected shape. This cohort remains failed evidence and supports no
acceptance or comparative-efficiency claim.

The [second diagnostic pair](../tests/agent-eval/results/2026-09-14-deferred-diagnostic-2/manifest.json)
confirmed the isolation fix. Both arms passed project and receiver oracles, exact reversal, ordered
checks and index preservation. The file arm passed acceptance. The `fr` arm repeated one unchanged,
successful batch preview, so the strict single-execution predicate rejected it. Instrumented steps
now refuse an exact successful request until a source mutation or rewritten artifact changes its
inputs, telling the agent to reuse the retained response.

The [accepted rerun](../tests/agent-eval/results/2026-09-14-deferred-final/manifest.json) binds the
same model settings to commit `5839cde`. Both arms pass the coordinated task, all project and
receiver oracle cases, exact reversal, ordered checks and index preservation. The `fr` arm uses
17,711 measured context tokens and 42 calls. The file arm uses 11,182 tokens and 17 calls. This one
pair confirms adoption of the corrected protocol and leaves broader context efficiency open.

The command shape follows the official [Codex non-interactive command reference](https://learn.chatgpt.com/docs/developer-commands?surface=cli#codex-exec) and [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference). `--ignore-user-config` still uses the operator's Codex home for authentication. Model availability and quota remain account-dependent; a failed launch stays part of the attempted trial record.
