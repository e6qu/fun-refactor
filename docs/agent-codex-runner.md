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

The pinned upstream rename task follows guide, preview, review and execute on three Rust files.
Its score additionally checks exact tracked source, all delivery stages, receiver patch replay and
64 independent compiled behavior cases:

```sh
python3 tools/upstream-rename-agent.py prepare /tmp/fr-upstream-rename --fr target/debug/fr
python3 tools/upstream-rename-agent.py run /tmp/fr-upstream-rename --confirm-agent-spend
python3 tools/upstream-rename-agent.py score /tmp/fr-upstream-rename
python3 tools/upstream-rename-agent.py record /tmp/fr-upstream-rename \
  tests/agent-eval/results/DATE-upstream-rename-acceptance
python3 tools/upstream-rename-agent.py audit \
  tests/agent-eval/results/DATE-upstream-rename-acceptance
```

The pinned MIT React/Tailwind task follows a TSX surface guide. Install dependencies from its
retained `pnpm-lock.yaml` with pnpm 7.33.7 before preparing a session. The agent's declared check
runs TypeScript without writing files; the score separately replays the patch, builds the receiver
and checks generated Tailwind CSS:

```sh
mkdir -p /tmp/fr-react-mit
tar -xzf tests/agent-eval/react-workspace.tar.gz -C /tmp/fr-react-mit
(cd /tmp/fr-react-mit && npx --yes pnpm@7.33.7 install --frozen-lockfile --ignore-scripts)
python3 tools/upstream-react-agent.py prepare /tmp/fr-upstream-react \
  --fr target/debug/fr --deps /tmp/fr-react-mit/node_modules
python3 tools/upstream-react-agent.py run /tmp/fr-upstream-react --confirm-agent-spend
python3 tools/upstream-react-agent.py score /tmp/fr-upstream-react
python3 tools/upstream-react-agent.py record /tmp/fr-upstream-react \
  tests/agent-eval/results/DATE-upstream-react-acceptance
python3 tools/upstream-react-agent.py audit \
  tests/agent-eval/results/DATE-upstream-react-acceptance
```

The pinned standalone CSS task reuses the React archive and its lockfile dependencies. Its
declared PostCSS check reads the stylesheet without writing source. The independent receiver
builds the project and verifies the generated selector:

```sh
python3 tools/upstream-css-agent.py prepare /tmp/fr-upstream-css \
  --fr target/debug/fr --deps /tmp/fr-react-mit/node_modules
python3 tools/upstream-css-agent.py run /tmp/fr-upstream-css --confirm-agent-spend
python3 tools/upstream-css-agent.py score /tmp/fr-upstream-css
python3 tools/upstream-css-agent.py record /tmp/fr-upstream-css \
  tests/agent-eval/results/DATE-upstream-css-acceptance
python3 tools/upstream-css-agent.py audit \
  tests/agent-eval/results/DATE-upstream-css-acceptance
```

The pinned authored TSX body task uses those same React dependencies. The agent follows an
explicit source body guide, receives one bounded declaration and submits a complete `Layout` body.
The independent receiver checks exact source, patch replay, the TypeScript/Vite build and three
server-rendered layouts:

```sh
python3 tools/upstream-tsx-body-agent.py prepare /tmp/fr-upstream-tsx-body \
  --fr target/debug/fr --deps /tmp/fr-react-mit/node_modules
python3 tools/upstream-tsx-body-agent.py run /tmp/fr-upstream-tsx-body --confirm-agent-spend
python3 tools/upstream-tsx-body-agent.py score /tmp/fr-upstream-tsx-body
python3 tools/upstream-tsx-body-agent.py record /tmp/fr-upstream-tsx-body \
  tests/agent-eval/results/DATE-upstream-tsx-body-acceptance
python3 tools/upstream-tsx-body-agent.py audit \
  tests/agent-eval/results/DATE-upstream-tsx-body-acceptance
```

The pinned Micromaid task follows a Markdown diagram guide and renames one Mermaid node. Prepare
its parser dependencies from the retained lockfile before the live session. The declared check
parses both diagrams without writing source; the score separately checks exact graph structure and
receiver patch replay:

```sh
mkdir -p /tmp/fr-mermaid-oracle
cp tests/agent-eval/mermaid-oracle/package*.json /tmp/fr-mermaid-oracle/
(cd /tmp/fr-mermaid-oracle && npm ci --no-audit --no-fund)
python3 tools/upstream-mermaid-agent.py prepare /tmp/fr-upstream-mermaid \
  --fr target/debug/fr --deps /tmp/fr-mermaid-oracle/node_modules
python3 tools/upstream-mermaid-agent.py run /tmp/fr-upstream-mermaid --confirm-agent-spend
python3 tools/upstream-mermaid-agent.py score /tmp/fr-upstream-mermaid
python3 tools/upstream-mermaid-agent.py record /tmp/fr-upstream-mermaid \
  tests/agent-eval/results/DATE-upstream-mermaid-acceptance
python3 tools/upstream-mermaid-agent.py audit \
  tests/agent-eval/results/DATE-upstream-mermaid-acceptance
```

Use `record --diagnostic --reason TEXT` for a scored failure or interrupted run. An interrupted
run may have no score or run record; its raw prompt and Codex stream remain diagnostic only.
