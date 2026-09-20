# Local Codex agent evaluation

The Codex evaluators run prepared agent tasks through `codex exec`. They are opt-in, never run in
normal CI and require an explicit quota acknowledgement. Use them to answer a defined acceptance or
comparison question after deterministic replay and scoring already pass.

The generic paired runner prepares isolated project copies and one immutable `fr` binary:

```sh
python3 tools/agent-eval.py prepare \
  --project regex-coordinated --repetitions 1 --out /tmp/fr-agent-sessions

python3 tools/agent-eval-codex.py /tmp/fr-agent-sessions \
  --trial regex-escape-len-fr --trial regex-escape-len-files --dry-run

python3 tools/agent-eval-codex.py /tmp/fr-agent-sessions \
  --trial regex-escape-len-fr --trial regex-escape-len-files --confirm-agent-spend
```

Each trial uses a new ephemeral Codex process with user configuration and repository discovery
disabled. The runner pins its model, reasoning effort, service tier and sandbox. It reads the frozen
prompt over standard input and refuses incomplete pairs or reused session directories. Model
availability and quota remain account-dependent.

A session retains its JSONL events, stderr, final response and run record. The run record binds the
prompt and streams by SHA-256 and records the model, effort, service tier, elapsed time and exit
status. Score both arms before retaining a cohort:

```sh
target/agent-eval-venv/bin/python tools/agent-eval.py score \
  /tmp/fr-agent-sessions/regex-escape-len-fr
target/agent-eval-venv/bin/python tools/agent-eval.py score \
  /tmp/fr-agent-sessions/regex-escape-len-files
target/agent-eval-venv/bin/python tools/agent-eval.py record \
  /tmp/fr-agent-sessions tests/agent-eval/results/DATE-context \
  --implementation-commit COMMIT --execution-note 'Exact execution conditions'
```

The manifest records acceptance, failures, source and evaluator identities, prompts, tool events,
settings and available usage fields. Failed cohorts remain diagnostic evidence and must not support
an acceptance or efficiency claim. Replay does not contact the model service.

Task-specific evaluators may add stronger isolation and oracles. The completion, matched-context and
matched-source runners freeze their own binaries, verify exact outputs and retain offline replay
commands. See [evaluation evidence](evaluations.md) for accepted results, diagnostic history,
interpretation limits and current economical settings.

The pinned upstream read/trace task runs one guided agent against the retained regex workspace.
It accepts only instrumented guide, follow, bounded find/show and finish requests. The score
checks exact source evidence, the cross-crate edge, unchanged source bytes and the Codex tool log.

```sh
python3 tools/upstream-read-agent.py prepare /tmp/fr-upstream-read --fr target/debug/fr
python3 tools/upstream-read-agent.py run /tmp/fr-upstream-read --confirm-agent-spend
python3 tools/upstream-read-agent.py score /tmp/fr-upstream-read
python3 tools/upstream-read-agent.py record /tmp/fr-upstream-read \
  tests/agent-eval/results/DATE-upstream-read-acceptance
python3 tools/upstream-read-agent.py audit \
  tests/agent-eval/results/DATE-upstream-read-acceptance
```

Use `record --diagnostic --reason TEXT` for a scored failure or interrupted run. An interrupted
run may have no score or run record; its raw prompt and Codex stream remain diagnostic only.
