# Public fr editing through native OpenCode tools

Four attempts: one reused signal-name validation task, two configured models and two tool arms.
This reviewed integration task appeared in the preceding pilot. It is not independent or held out.
Both arms retain ordinary edits. The fr arm adds optional body previews, reviewed history
application and a frozen usage guide. Account for that guide and all author output.

Implementation commit: `1fbdd4f152a0fa1d3c2afc64df95dba42ed92f8e`. Plan: `5217e3521e71dc2b78de5bd787b6631db8c321f83cca13cce05658a3a4f699dd`.

Commit this plan and runner before calls. Run every cell once in frozen order, serially
under the workstation guard. Keep every failure; do not increase budgets or retry cells.
Each attempt retains the existing 120-second wall, 20-second sampled CPU, 768 MiB RSS,
16 MiB growth, 24-call and 12-step limits. Candidate code and graders run only on GitHub.
Collection retained three submissions and one timeout. All three submissions passed the frozen
behavior checks on GitHub, with nine cases per submission.

Full provider billing/context and source repeated inside diffs remain incomplete.
Offline replay verifies byte changes; it does not rerun the frozen fr language parser.

| Model | Tools | Collection | Calls | fr calls | Author applies | Result bytes | Edit argument bytes | Seconds | Peak RSS MiB |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|
| Kimi | fr | submitted | 6 | 0 | 0 | 3281 | 461 | 65.7 | 630.3 |
| Kimi | files | submitted | 7 | 0 | 0 | 16052 | 1171 | 59.2 | 673.3 |
| GLM | fr | submitted | 10 | 0 | 0 | 5626 | 407 | 100.6 | 644.2 |
| GLM | files | failed | unknown | 0 | unknown | unknown | unknown | 120.0 | 619.6 |

The fr arm adds 593 instruction bytes and 1,384 schema bytes. Those costs apply even
when the agent chooses ordinary edits. Tool output and elapsed time alone are not total
context or dollar costs. Complete original records and any failed attempts remain unchanged.
None of the attempts called fr, including the partial timeout log. Differences in source reads
cannot be credited to fr. This pilot establishes no efficiency advantage or model adoption of
the public edit route. Real CLI controls establish the adapter's preview/apply behavior separately.

## Separate behavior grading

The [grading job](https://github.com/e6qu/fun-refactor/actions/runs/37010478080/job/110848739512)
passed on commit `4e1195d875ac7790ee1b96fb916a5fa39fe9ad5c`.
The retained `github-grades.json`, `github-controls.json` and `github-grading.json` identify
the exact submissions, frozen grader, case results and GitHub run. Candidate inventories and
stdout/exit results match the frozen inputs. Both unchanged-source controls failed, both reference
fixes passed, and all six wrong-fix controls failed as expected.

Original collection records remain unchanged, including their pending grade status at collection
time. These separate reports establish three bounded behavior passes and one collection timeout.
They do not establish general correctness, an efficiency advantage or adoption of the fr edit route.
