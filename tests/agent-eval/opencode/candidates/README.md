# Candidate tasks for a broader code-change comparison

These three tasks are prepared for review. No agent has attempted them in this pack, and no
independent reviewer has accepted the requirements or graders. They do not establish an efficiency
advantage, a held-out evaluation or a completed roadmap milestone.

| Task | Pinned source | Requested behavior | Investigation spans |
| --- | --- | --- | --- |
| `dotenv-alternate` | python-dotenv, `0b288059` | Add `${NAME:+word}` while preserving interpolation precedence and existing substitutions | Public loading API, file parsing, variable atoms and environment resolution |
| `packaging-prerelease` | packaging 24.2, `d8e3b31b` | Select prereleases only when the full intersection has no matching final release, unless explicitly enabled | Set filtering, individual comparisons and version classification |
| `platformdirs-xdg` | platformdirs 4.3.6, `bc0405cb` | Ignore relative XDG paths consistently across directory APIs | Unix properties, shared path helpers, public wrappers and iterators |

The task prompts name observable behavior without identifying edit locations. A correct repair may
change one file; the task asks the agent to investigate behavior across modules, not meet a file-count
quota. The source projects are new to the retained native change cohorts. Model training exposure
is unknown. Packaging and platformdirs use older releases; newer upstream code already addresses
related behavior. Task preparation included inspecting newer source. These are candidate regression
tasks, not discoveries of current upstream bugs or untouched holdouts.

## What is frozen here

`manifest.json` records full revisions, requirements, archive hashes, model IDs and paired allocation
settings. `sources.json` records every selected file and its hash. The archives contain unmodified
runtime packages, licenses and selected root metadata. Upstream tests, other documentation and
development files are omitted. They are bounded source subsets, not complete checkouts.

The platformdirs Git tree omits its build-generated `version` module. Both check commands provide
only the release's version metadata before importing the package. Directory behavior comes entirely
from the submitted source. There is no package installation or dependency download inside a check.

`public-checks.json` contains one visible example per task. The three private grader files contain
22 cases in total, covering boundaries and preserved behavior. Private commands and expected results
remain outside the agent's source bundle and tool surface. Publishing this pack makes them inspectable
by reviewers; they are not confidential once published.

`controls.json` retains reference edits and five incomplete repairs per task. Edits replay through
the ordinary native edit tool with a fresh source hash at every step. `control-failures.json` names
the exact private cases each variant must fail. Every incomplete repair is expected to pass its
public example and fail private cases. A syntax error or broken import cannot substitute for the
declared pattern of failures.

Six additional controls target omissions in the original graders. Each must pass its unchanged
public example and the frozen private grader in `review-baseline/`, then fail exactly its named
case in the strengthened grader. Both grades bind to the same submitted source and pinned image.

| Task | Incomplete repair | Newly distinguishing case |
| --- | --- | --- |
| dotenv | Erase ordinary substitutions | `ordinary-values` |
| dotenv | Give process values precedence in `dotenv_values` | `values-precedence` |
| packaging | Accept every version in `SpecifierSet.contains` | `contains-policy` |
| packaging | Return nothing from individual `Specifier.filter` | `individual-filter` |
| platformdirs | Create directories before rejecting relative entries | `site-creation` |
| platformdirs | Also reject relative runtime-directory values | `unrelated-directories` |

These are task-author counterexamples, not independent review findings. The bounded
[OpenCode review attempts](../reviews/2026-10-03-candidates/README.md) retained two timeouts and
no completed review. The other four planned reviews were not started.
The follow-up [source-based reviews](../reviews/2026-10-04-native/README.md) also stopped after
two timeouts. They retained 17 source-tool calls but no submitted findings; independent review
remains pending.

## Validate on GitHub

Each task has its own five-minute CI job. It compares the archive's file bytes and executable modes
against GitHub's pinned Git tree, then grades unchanged source, the reference and five incomplete
repairs. That is 21 code variants and 207 container case executions across the pack: 154 strengthened
private cases, 21 public examples and 32 original private cases for the six new controls. Jobs retain
public, current and original private grades, source and upstream tree identities as artifacts, including
when a control fails. Containers use the existing pinned image and isolated grader limits.

```sh
python3 tools/check-code-change-trials.py \
  --task-pack tests/agent-eval/opencode/candidates \
  --task dotenv-alternate --controls-only --verify-upstream \
  --output target/candidate-controls
```

Run this command only on GitHub. Local `test_change_controls.py` checks identities, edit replay,
syntax and frozen-plan compatibility without importing or executing candidate code. Use the
workstation guard for that check.

## Before live change trials

An independent reviewer still needs to assess task clarity, realistic scope, baseline failures,
reference behavior, plausible alternative repairs and grader blind spots. The same author selected
the tasks, wrote the reference repairs and wrote these graders; their agreement is not independent
validation. Record that review and any changes before freezing a live collection.

After review, freeze through `tools/native-changes.py` with this manifest and `--public-checks`.
Keep both ordinary and fr arms, all failed attempts and the existing attempt limits. Do not require
fr calls or expand budgets because a task times out. Protocol 3 needs OpenCode provider access on a
GitHub runner; locally configured Kimi/GLM access does not establish that access. The recorded
review calls did not change provider configuration. Delegation, proof tasks, complete cost accounting
and a matched independent comparison remain open.
