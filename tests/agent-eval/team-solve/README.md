# A live Luna, GLM and Kimi team repaired one small Python function

The first pilot ran on 2026-11-01. The input returned two pages for eleven items
at five per page. The team delivered the one-line repair in [pilot.patch](pilot.patch);
both declared check phases passed all eight supplied cases. The original project
and checker stayed unchanged. This is a Python integer exercise, separate from
the Rust page-count Lean proof task.

| Role | Agent | Complete role time |
| --- | --- | ---: |
| Plan | Luna Fast | 19.8 seconds |
| First proposal | GLM Flash | 71.6 seconds |
| Independent second proposal | Kimi K3 | 41.2 seconds |
| Implement | Luna Fast | 23.3 seconds |
| Review actual checks and code | Kimi K3 | 84.1 seconds |
| Repair | Skipped because initial checks passed | No session |

The complete run took **249.9 seconds**, including preparation and checks.
All five native calls finished within their separate 150-second role limits.
The total team ceiling was 1,200 seconds. This did not extend or replace the
existing 150-second solo comparison. There was no timed solo baseline, so this
pilot establishes neither a speed advantage nor a quality improvement.

The later [timing review](pilot-time-review.json) breaks down that elapsed time.
Planning and implementation took about 43 seconds; proposals and review took
about 197 seconds, or 79% of the total. Both declared check phases together took
about 0.04 seconds. The remaining ten seconds cover preparation and coordination.
The implementation stage alone is not a complete solo baseline: no such run was
measured. Role time includes CLI activity and waiting, rather than measuring
provider computation time.

The [reviewed summary](pilot-summary.json) records each role's reported usage,
the frozen inputs and limits, the public patch hash, and the pre-run protocol hash.
OpenCode2 session exports confirmed the requested GLM and Kimi models and low
reasoning settings. Codex was configured for Luna Fast; its turn report does not
independently identify the serving model. Actual account charges remain unknown.
The original summary's `pilot.model_calls: 5` counts five native CLI sessions.
Each session can make multiple provider requests while using tools; their total
request count is unknown. The timing review records this clarification without
changing the original outcome or source record.

Luna and GLM initially tried to read source files from the handoff folder, then
recovered. The next prompt explicitly says that source and test paths are relative
to the project root. No role actually invoked fr in this tiny repair. Availability
alone establishes no fr contribution or measured benefit.

Raw prompts, handoffs, CLI output, source archives and session exports remain
local. Only this reviewed summary and submitted patch are published.
The scripted controls independently apply the retained patch to the original
fixture: the supplied test must reject the original and accept the patched function.
They also cover a failed initial check followed by one repair, protected claims,
rejected reviewer edits and retained resource failures, without model calls.

Follow the [complete setup guide](../../../docs/team-solve.md) for explicit role
assignments, budgets and checker settings. This pilot does not close M3 or change
any earlier agent outcome. A future comparison needs the same frozen tasks and
checks, explicit solo and team budgets, and all failures retained under the
[improvement method](../../../docs/agent-improvement.md).
The runner now also offers one agent session and Luna in every team role as
explicit comparison conditions. Their scripted checks establish their session sequence, protected
inputs and shared run deadline. No paid comparison of those conditions has run.
