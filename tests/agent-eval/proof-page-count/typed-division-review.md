# Check the proof step, not just the theorem name

The no-model [control report](typed-division-controls.json) uses the complete
original page-count claim. Eleven items at five per page must give three pages;
invalid inputs return `-1`, and valid signed-64 inputs have the original bounds.
The model still uses mathematical integers. These controls do not prove that the
Rust implementation matches the model.

Lean accepted the reference before and after seven controlled mistakes. Each
rejection included a location in the submitted draft. No agent was called, and
no original attempt changed outcome.

| One changed step | What Lean reported |
| --- | --- |
| Pass `items` where `Int.tdiv_nonneg` expects a proof | An integer has type `Int`; the required argument is a proof of an inequality |
| Pass the upper input bound instead of nonnegativity | The supplied inequality has the wrong type |
| Ask `omega` for quotient nonnegativity without the division fact | The arithmetic goal remains unproved |
| Supply nonnegativity of Euclidean division for truncating division | The resulting expression has the wrong type |
| Remove conversion from truncating to Euclidean division | The strict-bound theorem proves a different expression |
| Unpack invalid-input alternatives as a pair | The case split fails |
| Ask `omega` for the strict quotient bound without the division theorem | The arithmetic goal remains unproved |

For example, `items` is an integer. `hi0 : 0 ≤ items` is a proof about that
integer. `Int.tdiv_nonneg hi0 (by omega : 0 ≤ size)` needs the latter.
Finding `Int.tdiv_nonneg` does not make `Int.tdiv_nonneg items size` valid.
The full returned diagnostic includes both the actual and expected types.

This makes applying facts a concrete obstacle worth testing. It does not tell
us how much of an agent's elapsed time that obstacle caused, or whether a new
instruction will improve success. The earlier reviewed agent actions remain the
evidence that these mistakes occurred during actual attempts.

## Prediction before another comparison

The proposed single instruction change is: after a type error, read the mapped
error's context, match each required proof argument to a named assumption, and
check that one step before searching for another theorem. Supply no reference
proof, new theorem names or stronger assumptions.

A fresh comparison would give both conditions the same complete page-count
claim, current client, fr executable and shared skill. One condition adds only
that instruction. Freeze fresh complete admission, all eight slots and the
decision rule before calls: GLM and Kimi, two conditions, two repetitions each.
Keep the 150-second complete-attempt deadline and all resource limits.
Never reopen an earlier slot or reuse the old client's admission.

The prediction is that each agent checks a draft within 45 seconds and delivers
at least one independently accepted complete proof in its two instructed attempts.
Missing times or incomplete accounting cannot satisfy it. Count complete results
per agent and condition; a two-versus-zero result supports testing the instruction
further, while ties and mixed results leave its benefit unresolved. A prediction
failure keeps the broader comparison paused.

This is a proposal, not an execution plan or authorization receipt. The current
PR contains no live comparison. Keep M3 open and keep its remaining work together
in its milestone PR.
