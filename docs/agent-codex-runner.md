# Local Codex agent evaluation

`tools/agent-eval-codex.py` launches prepared paired trials through `codex exec`. It is opt-in, never runs from the normal CI gate and requires an explicit quota acknowledgement.

Prepare a fresh pair, inspect the generated prompts, then dry-run the exact commands:

```sh
python3 tools/agent-eval.py prepare --project regex-coordinated --repetitions 1 --out /tmp/fr-agent-sessions
python3 tools/agent-eval-codex.py /tmp/fr-agent-sessions --trial regex-escape-len-fr --trial regex-escape-len-files --dry-run
```

Run the reviewed pair locally:

```sh
python3 tools/agent-eval-codex.py /tmp/fr-agent-sessions --trial regex-escape-len-fr --trial regex-escape-len-files --confirm-agent-spend
```

Each trial is a new `codex exec --ephemeral` invocation. The runner passes `--ignore-user-config` and `--ignore-rules`, pins `gpt-5.6-luna`, `model_reasoning_effort="low"`, `service_tier="default"` and the workspace-write sandbox, and reads the frozen prompt over standard input. It accepts only complete fr/files pairs from one prepared experiment and refuses a session with existing run or harness events.

The session retains `codex-events.jsonl`, `codex-stderr.txt`, `codex-final.txt` and `codex-run.json`. The run record binds the prompt and streams by SHA-256 and records model, effort, service tier, elapsed time and exit status. Harness events, scoring, independent oracles and final evidence recording remain under `tools/agent-eval.py`.

The command shape follows the official [Codex non-interactive command reference](https://learn.chatgpt.com/docs/developer-commands?surface=cli#codex-exec) and [configuration reference](https://learn.chatgpt.com/docs/config-file/config-reference). `--ignore-user-config` still uses the operator's Codex home for authentication. Model availability and quota remain account-dependent; a failed launch stays part of the attempted trial record.
