# Compare agent work on a real task

Use this evaluation runner to compare how an agent solves the same task with ordinary tools and with `fr`.
For example, both attempts should fix the same bug and face the same independent tests. These optional runs need explicit cost and resource approval; current stopped studies stay stopped.

An attempt is one agent trying one task with one tool setup.
The baseline setup uses ordinary search, file reads and edit tools; the compared setup also offers `fr`.
A grader checks the submitted answer or code against requirements selected before the attempt.
For a rounding repair, give both setups the same bug and test their submitted code with the same independent rounding cases.
The runner itself adds setup and recording work; include that boundary when interpreting its time totals.

## Commands, results and limits

The Codex evaluators run prepared agent tasks through `codex exec`. They are opt-in, never run in
normal CI and require an explicit quota acknowledgement. Use them to answer a defined acceptance or
comparison question after deterministic replay and scoring already pass.

The generic paired runner prepares isolated project copies and one immutable `fr` binary.
After the settings and spending limits have been agreed, save the chosen settings as `/tmp/fr-codex-settings.json`:

```json
{
  "model": "MODEL_ID_FROM_THE_AGREED_PLAN",
  "reasoning_effort": "AGREED_EFFORT",
  "service_tier": "AGREED_SERVICE_TIER"
}
```

Replace those labels with actual agreed values; they are not model recommendations.
A service tier selects the provider's requested processing class. Use the same explicit value for both setups.
The preparation command copies these settings into every session before an agent can run.
Without the file, preparation creates model-free sessions for scripted checks; the Codex runner rejects them.
Set `CODEX_BINARY` and `FR_BINARY` to the reviewed executables' absolute paths, and use those same agreed settings in the run commands:

```sh
python3 tools/agent-eval.py prepare \
  --fr "$FR_BINARY" --project regex-coordinated --repetitions 1 \
  --codex-settings /tmp/fr-codex-settings.json --out /tmp/fr-agent-sessions

python3 tools/agent-eval-codex.py /tmp/fr-agent-sessions \
  --codex "$CODEX_BINARY" --model MODEL_ID_FROM_THE_AGREED_PLAN \
  --effort AGREED_EFFORT --service-tier AGREED_SERVICE_TIER --timeout 900 \
  --trial regex-escape-len-fr --trial regex-escape-len-files --dry-run

python3 tools/agent-eval-codex.py /tmp/fr-agent-sessions \
  --codex "$CODEX_BINARY" --model MODEL_ID_FROM_THE_AGREED_PLAN \
  --effort AGREED_EFFORT --service-tier AGREED_SERVICE_TIER --timeout 900 \
  --trial regex-escape-len-fr --trial regex-escape-len-files --confirm-agent-spend
```

Each trial uses a new ephemeral Codex process. Its [`codex exec` flags](https://learn.chatgpt.com/docs/developer-commands?surface=cli)
ignore the user configuration file and execution-policy rule files. The runner pins its model, reasoning effort, service tier and sandbox. It reads the frozen
prompt over standard input and refuses incomplete pairs or reused session directories. Model
availability and quota remain account-dependent.
Model, reasoning effort, service tier, executable and timeout have no runner defaults.
Undeclared or mismatched session settings fail before creating attempt records or contacting a model.
The example states the existing maximum of 900 seconds explicitly; it does not extend it.

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

For the bug example, read both attempts' independent scores before comparing their usage.
If one returns a smaller answer but fails the regression test, report that failure with its cost.
If billing is incomplete, keep the dollar comparison unknown. A dry run prepares commands; it is
not a completed agent attempt.

Task-specific evaluators may add stronger isolation and oracles. The completion, matched-context and
matched-source runners freeze their own binaries, verify exact outputs and retain offline replay
commands. See [evaluation evidence](evaluations.md) for accepted results, diagnostic history,
interpretation limits and current economical settings.

The [unknown-target runner](unknown-target-investigations.md) compares symptom/requirement-only
tasks with ordinary source editing. Each trial has two fresh processes separated by a dependency
change, retained handoff and stale-review check. Its deterministic rehearsal must pass before a live
trial, and interrupted attempts remain diagnostic evidence. It uses the local CLI's ephemeral JSON
execution mode; see [non-interactive execution](https://learn.chatgpt.com/docs/non-interactive-mode)
for the underlying Codex interface.

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

The paired TSX body task selects `Layout` and `Header` through one `source-bodies` goal. The agent
receives two bounded declarations and submits two braced bodies without their signatures. One
batch preview and one native review bind both paths. The receiver checks exact source, patch
replay, the TypeScript/Vite build and both landmarks in three server-rendered layouts:

```sh
python3 tools/upstream-multibody-agent.py prepare /tmp/fr-upstream-multibody \
  --fr target/debug/fr --deps /tmp/fr-react-mit/node_modules
python3 tools/upstream-multibody-agent.py run /tmp/fr-upstream-multibody --confirm-agent-spend
python3 tools/upstream-multibody-agent.py score /tmp/fr-upstream-multibody
python3 tools/upstream-multibody-agent.py record /tmp/fr-upstream-multibody \
  tests/agent-eval/results/DATE-upstream-multibody-acceptance
python3 tools/upstream-multibody-agent.py audit \
  tests/agent-eval/results/DATE-upstream-multibody-acceptance
```

The application migration task reads a validated Express route through application IR and writes
a Go standard-library HTTP adapter. Its declared Go check uses a session-local build cache. The
score replays the patch in a fresh receiver and exercises accepted, noncanonical, duplicate,
missing, mistyped and malformed requests:

```sh
python3 tools/application-migration-agent.py prepare /tmp/fr-application-migration \
  --fr target/debug/fr
python3 tools/application-migration-agent.py run /tmp/fr-application-migration \
  --confirm-agent-spend
python3 tools/application-migration-agent.py score /tmp/fr-application-migration
python3 tools/application-migration-agent.py record /tmp/fr-application-migration \
  tests/agent-eval/results/DATE-application-migration-acceptance
python3 tools/application-migration-agent.py audit \
  tests/agent-eval/results/DATE-application-migration-acceptance
```

The guided Lean task exposes the exact theorem and proof contract before accepting agent-authored
tactics. The score recreates the unproved package, replays the reviewed patch and runs strict
verification in a fresh receiver:

```sh
python3 tools/proof-authoring-agent.py prepare /tmp/fr-proof-authoring --fr target/debug/fr
python3 tools/proof-authoring-agent.py run /tmp/fr-proof-authoring --confirm-agent-spend
python3 tools/proof-authoring-agent.py score /tmp/fr-proof-authoring
python3 tools/proof-authoring-agent.py record /tmp/fr-proof-authoring \
  tests/agent-eval/results/DATE-proof-authoring-acceptance
python3 tools/proof-authoring-agent.py audit \
  tests/agent-eval/results/DATE-proof-authoring-acceptance
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
