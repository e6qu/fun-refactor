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
Collection is pending. A submission is not a behavior pass.

Full provider billing/context and source repeated inside diffs remain incomplete.
Offline replay verifies byte changes; it does not rerun the frozen fr language parser.
