# Unknown-target investigation trials

The [task contract](../tests/agent-eval/unknown-target/task.json) pins a Unicode similarity bug in
the published strsim crate and a cross-crate feature in the regex workspace. Prompts supply symptoms
or API requirements. They do not supply edit paths, existing private helper names or a prescribed
discovery sequence. The archives and independent behavioral oracles predate this evaluation.

Each task has an `fr` arm and an ordinary source arm. Both use the same source revision, requirement,
compiler checks and independent oracles. The ordinary arm searches and reads source, then authors
exact replacements or appended declarations. The `fr` arm discovers handles and authors structural
changes. Both inspect a complete review and use an automated check/apply/reverse/reapply lifecycle.
This compares investigation and authoring surfaces; it does not compare manual Git administration.

## Interruption and evidence

Discovery ends with a reviewed proposal and an agent-authored handoff: diagnosis, cited discovery
events, selected source dependencies and pending work. No source change has executed. The evaluator
then appends an inert comment to one selected dependency and attempts the old review. That attempt
must refuse without changing source.

The `fr` arm also persists a native task plan through the verified object store. Resumption must
invalidate the diagnosis while retaining an independent documentation observation. Observation
evidence records an agent hypothesis; it is not an independent correctness certificate. The pending
delivery step cannot become satisfied from the stale diagnosis.

A second fresh agent process receives the requirement, handoff and intervening edit. It must inspect
current evidence and author a new review before executing. Both arms receive the same kind of source
change. The comment exercises source dependency invalidation, not semantic change understanding.

## Reproduction

Build the current native binary with one compiler worker. Prepare the pinned regex dependencies
using the development guide before offline runs. The deterministic rehearsal supplies the solution
and tests the harness; it must never count as an agent result:

```sh
python3 tools/investigation-agent.py rehearse /tmp/fr-investigation-rehearsal \
  --fr target/debug/fr --task unicode-dice --arm fr
```

Prepare separate fresh sessions for both arms and both tasks. These commands prepare and run one arm:

```sh
python3 tools/investigation-agent.py prepare /tmp/fr-investigation-bug-fr \
  --fr target/debug/fr --task unicode-dice --arm fr
python3 tools/investigation-agent.py run /tmp/fr-investigation-bug-fr --confirm-agent-spend
python3 tools/investigation-agent.py resume /tmp/fr-investigation-bug-fr
python3 tools/investigation-agent.py run /tmp/fr-investigation-bug-fr --confirm-agent-spend
python3 tools/investigation-agent.py score /tmp/fr-investigation-bug-fr
python3 tools/investigation-agent.py record /tmp/fr-investigation-bug-fr \
  tests/agent-eval/results/DATE-investigation-bug-fr
```

Use `--arm files` for ordinary discovery and `--task regex-escape-len` for the feature. Live execution
requires the quota acknowledgement. Run one worker at a time. Each phase has a 20-minute timeout,
a 16 MiB transcript budget and a sampled 3 GiB process-group RSS limit. A trial admits 80 tool calls
and 1 MiB of visible reports. Cleanup targets only the runner's owned process group.

`record --diagnostic` retains an unsuccessful or interrupted session. A phase cannot be retried in
place. Preserve failed attempts before preparing a fresh trial. Preparation and scoring never call
the model service. `audit DIRECTORY` validates retained file and evaluator identities;
`replay DIRECTORY` also recreates the pinned source, applies the patch, repeats compiler and behavioral
checks, and verifies exact reversal and reapplication.

## Interpretation

The instrumented boundary is cooperative. The scorer checks the Codex command stream against the
tool log; merely including the allowed command in a shell script is insufficient. Raw source commands,
unaccounted tool calls and malformed transcripts cannot establish accepted comparison evidence.
The runner freezes its binary and binds the evaluator and SDK sources. Retained source snapshots keep
historical results auditable after later implementation changes.

Report complete and failed trials, disclosed bytes, source reveals, tool calls, elapsed time and
available Codex usage. Usage is not billed quota. Sampled group RSS can miss peaks and measures the
agent process and its same-group children, not every possible descendant.

These Rust tasks use syntax discovery, compiler checks and executable behavioral oracles. They do
not extend the Python scalar dataflow subset or prove source implementation equivalence. Two tasks
on two pinned repositories cannot establish general investigation success or close every roadmap gate.
