# Let Luna, GLM and Kimi work on one checked change

Suppose `pages(11, 5)` returns two, but eleven items need three pages.
One agent can propose a fix; another can look for mistakes. This local runner
lets our three selected agents do that through short files and fresh CLI sessions.
The final result is a patch for you to review. Your original project stays unchanged.

This follows the sequence in Memdoor's
[team-solve example](https://github.com/guregodevo/memdoor/tree/597c2cf641aec10e7c4cbf0f28be5236ef3cc91d/examples/workflows/team-solve).
Our implementation uses the installed Codex and OpenCode2 commands directly.
It does not install Memdoor or send credentials to another service.

## Who does what

A role is one fresh agent session with one specific job. A handoff is the short
Markdown file that session leaves for the next relevant role. The runner supplies
the task and selected handoffs; it does not carry over a chat conversation.

| Step | Agent in this example | What it receives and produces |
| --- | --- | --- |
| Plan | Luna | Read the task; write a plan and identify needed checks |
| First proposal | GLM | Read the task and plan; propose an implementation |
| Second proposal | Kimi | Read the same task and plan, without seeing GLM's proposal |
| Implement | Luna | Read both proposals; edit allowed source and explain the choice |
| Initial checks | Ordinary programs | Run the exact commands you declared; keep their full bounded output |
| Review | Kimi | Read the changed source, handoffs, real check results and logs; explain failures |
| One repair, if needed | Luna | Run only when initial checks failed; use that failure and review |
| Final checks | Ordinary programs | Run the declared checks again and capture the final patch |

Roles run one at a time to protect local interactivity. Each gets its own project
copy. Planning, proposals and review cannot edit source. If the first checks pass,
repair is skipped: a reviewer's speculation cannot replace a passing candidate.
A resource limit stops the run for inspection. There is no model substitution,
automatic retry or second repair round.

Codex is pinned to 0.164.0 with `gpt-6-luna`, high reasoning and Fast service.
OpenCode2 is pinned to 2.0.32 with `zai-coding-plan/glm-5.3-flash#low`
and `kimi-code-plan-global/k3#low`. Settings must assign all six roles explicitly
using all three profiles. Each call uses the existing native-client checks;
OpenCode2's exported session must confirm its actual model selection and outcome.
Codex's usage report does not independently confirm which model served a response.

## Run the small Python repair

Use the repository checkout, Python 3, Git and the pinned CLIs, already signed in.
This local entry point requires this workstation's resource guard at
`/Users/zardoz/.codex/tools/fr-local-guard.py`. Each workload runs under that guard;
do not wrap the whole multi-stage command in a second guard.
The repository itself must be committed and clean so the runner can record its identity.

Also supply an `fr` executable built by the hosted M3 preparation workflow, with
its `source-commit.txt`, `source-tree.txt` and `fr.sha256` alongside it.
The runner checks that the build's native source matches this checkout.
Full builds and full test gates belong on GitHub runners.

The following creates a tiny independent Git project. Its supplied check covers
eight cases, including invalid inputs and a large integer. Read both the task
and check before running any agents. The deliberately broken function rounds down.

```sh
mkdir -p /private/tmp/fr-team-example
cp -R tests/agent-eval/team-solve/project /private/tmp/fr-team-example/project
git -C /private/tmp/fr-team-example/project init -b main
git -C /private/tmp/fr-team-example/project add counter.py check.py
git -C /private/tmp/fr-team-example/project commit -m 'Prepare the page-count repair example'
cat tests/agent-eval/team-solve/task.md
cat /private/tmp/fr-team-example/project/check.py
```

Create `settings.json` with the following Python snippet. Replace the three
executable paths and build directory with your actual absolute paths before
running it. `output` must be a new directory outside the input project.

```python
import json
from pathlib import Path
import sys

repo = Path.cwd().resolve()
settings = {
    "project": "/private/tmp/fr-team-example/project",
    "task": str(repo / "tests/agent-eval/team-solve/task.md"),
    "output": "/private/tmp/fr-team-example/run-1",
    "profiles": str(repo / "tests/agent-eval/proof-writing/profiles.json"),
    "binaries": {
        "fr": "/ABSOLUTE/PATH/TO/HOSTED/BUILD/fr",
        "codex": "/ABSOLUTE/PATH/TO/codex",
        "opencode2": "/ABSOLUTE/PATH/TO/opencode2"
    },
    "fr_build": "/ABSOLUTE/PATH/TO/HOSTED/BUILD",
    "roles": {
        "plan": "codex-luna-fast",
        "ideate-a": "opencode2-glm-flash",
        "ideate-b": "opencode2-kimi-k3",
        "solve": "codex-luna-fast",
        "review": "opencode2-kimi-k3",
        "repair": "codex-luna-fast"
    },
    "editable_files": ["counter.py"],
    "editable_regions": [],
    "protected_files": ["check.py"],
    "checks": [{
        "name": "page examples",
        "argv": [sys.executable, "-B", "check.py"],
        "cwd": ".",
        "timeout_seconds": 20,
        "covers": ["eight supplied Python page-count cases"],
        "verdict": "exit-code"
    }],
    "check_environment": {"PYTHONDONTWRITEBYTECODE": "1"},
    "reference_directories": [],
    "require_change": True,
    "limits": {
        "prepare_seconds": 60,
        "role_seconds": 150,
        "check_seconds": 120,
        "team_seconds": 1200,
        "parallel_roles": 1
    }
}
Path("/private/tmp/fr-team-example/settings.json").write_text(
    json.dumps(settings, indent=2) + "\n")
```

Run from the repository root:

```sh
PYTHONPATH=tools:sdk/python/src python3 -B -m agent_eval.team_solve run \
  --settings /private/tmp/fr-team-example/settings.json
cat /private/tmp/fr-team-example/run-1/summary.json
cat /private/tmp/fr-team-example/run-1/change.patch
```

The declared budgets are ceilings, not extra time added after a timeout.
Preparation has 60 seconds; each native role has at most 150 seconds including
setup and accounting; each check phase has 120 seconds across its commands.
The whole team has at most 1,200 seconds including preparation and handoffs.
There are five calls when initial checks pass, six when repair runs.
The existing guard separately enforces CPU, memory, free disk and per-workload time.
The child runner caps sampled RSS at 768 MiB, workspace growth at 512 MiB,
CPU at 75 seconds and combined command output at 1 MiB per workload.
These are sampled limits, not OS isolation from an adversarial program.

## Declare what may change and what counts as success

`editable_files` permits complete changes to those literal relative paths.
Other source paths stay fixed. `protected_files` explicitly names trusted
checking inputs and cannot overlap editable paths. Checks must use the copied
project: for example, `check.py` is relative to its root. Do not point a script
argument back at the original project's file. The executable must be absolute;
its hash is recorded and checked again before use. Review the script and any
dependencies yourself: the runner does not discover a complete trust boundary.

For an ordinary test command, choose `exit-code`: zero means success.
For `fr --json` commands that return a Boolean `passed` field, choose `json-passed`:
success requires both exit zero and exactly `"passed": true`. A successful
process reporting `"passed": false` fails. A malformed report also fails.
All declared checks must pass. `require_change: true` also requires a source change.
The runner installs those checks in `.fr/checks.json` for the agents and protects it.

For a Lean proof, use `editable_files: []` and an `editable_regions` entry such as:

```json
{"path": "specs/Claim.lean", "begin": "-- fr:proof-begin claim", "end": "-- fr:proof-end claim"}
```

Each marker must appear once on its own line. Only the lines between the markers
may change; the claim, assumptions and surrounding definitions must stay fixed.
List the test and project files in `protected_files`. Supply a pinned Lean checker
that rejects proof gaps, unapproved axioms and warnings, as in the
[proof workflow](agent-formalization.md). A marker boundary alone does not check
those proof requirements. If Lean proves a model, the connection to the original
source remains a separate claim. Declare any needed toolchain environment and
read-only Lean source directories explicitly.

## Inspect failures and compare fairly

`summary.json` contains outcomes, timing and reported usage without raw transcripts
or handoffs. `change.patch` is produced after final checks, including an ordinary
failed final check. Review these two files before publishing them.
Everything else stays local: task copies, source archives, prompts, handoffs,
native output and session exports. A failed role keeps its bounded source archive
and handoff when collection succeeds. An interrupted guard can leave its working
directory for inspection. Reusing a run directory is refused; inspect an attempt
before creating a new one. Credentials are never copied into run records.

The scripted controls exercise passing and failing candidates, independent proposals,
one repair, protected checking inputs and claims, actual JSON verdicts and resource
stops. They make no model requests. Run them locally only through the resource guard:

```sh
python3 /Users/zardoz/.codex/tools/fr-local-guard.py \
  env PYTHONPATH=tools:sdk/python/src python3 -B tools/agent_eval/test_team_solve.py -v
```

The [hosted controls](../.github/workflows/team-solve.yml) run the same checks;
the main PR checks include them too. Scripted success does not establish live model quality.

The [first live pilot](../tests/agent-eval/team-solve/README.md) repaired the small
Python example in 249.9 seconds across five calls. Both check phases passed;
repair was skipped. It used no actual fr command and had no timed solo baseline.
The reviewed record establishes working file handoffs and checked delivery,
without a measured fr benefit or evidence that the team improves on one agent.

Treat a team attempt as a separate experiment from the existing 150-second solo
comparison. Record all role time, failed calls, checker time and reported usage;
actual account charges remain unknown unless independently observed. A team may
spend five or six calls and substantially more time than one solo attempt.
To measure benefit, freeze the same task and checks, set explicit total budgets
for both approaches, keep every failure, and compare final accepted patches.
Do not count a team result as an M3 solo success. The GLM/Kimi page-count proof gap
and paused full comparison remain open.
