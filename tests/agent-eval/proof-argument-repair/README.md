# Page-count proof comparison

None of the eight attempts submitted a patch. Adding the repair instruction
produced no complete delivered proof in this comparison. One instructed Kimi
draft passed fr checking near 135 seconds, but the agent did not save it, check
the package or deliver a patch before its deadline. [Independent grading](https://github.com/e6qu/fun-refactor/actions/runs/41918858328)
checked all eight outcomes and confirmed zero successful submissions.

| Agent | Shared skill: delivered / 2 | Added instruction: delivered / 2 |
| --- | ---: | ---: |
| GLM Flash | 0 | 0 |
| Kimi K3 | 0 | 0 |

No mapped argument-type error was observed in the eight recorded attempts.
Some output was shortened by the agents or the diagnostic budget. This does not
establish that no such error occurred. The proposed repair mechanism remains
unobserved. Neither agent met the predeclared complete-proof prediction.
Do not adopt this instruction as an improvement or resume the 36-attempt study.
M3 remains open.

## What the agents had to do

Eleven items at five per page need three pages. The complete claim also says
that invalid inputs return `-1` and that valid results stay within the fixed
bounds. Each attempt started with the same Rust source, Lean definitions,
assumptions and unfinished theorem. Only the marked proof region could change.
An accepted proof would establish the mathematical model. Its connection to
Rust execution remains unproved.

Tactics are the proof steps given to Lean. A theorem can require another proof
as an argument. For example, `n : Int` is a number, while `hn : 0 ≤ n` proves
that the number is nonnegative. Supplying `n` where Lean requires `hn` is a
type error. The added instruction says to read that error, find or derive the
required proof, and check the repair before searching for another theorem.
It supplies no finished proof, extra theorem names or stronger assumptions.

The baseline is the same agent using the shared fr skill alone. Both settings
read the same handout path; only its contents differ. There are two repetitions
per agent and setting, in a fixed order that reverses between repetitions.
The [requirements](requirements.json) preserve the prediction and decision rules.
This comparison does not measure fr overhead against ordinary Lean tools.

Both agents used the local OpenCode2 2.2.5 application, low reasoning, fr 0.89.1,
Lean 4.28.0 and a copied Python 3.14.10 installation. Fresh conversations,
projects and fr caches were used. Operating-system and provider caches were
uncontrolled.

## What happened

GLM struggled with tactic structure, missing names and unpacking assumptions.
Its first whole-proof checks arrived about 67 to 139 seconds into the attempts.
Kimi used interactive goal inspection in three attempts. One complete draft
passed fr checking; other attempts stopped with unfinished goals or warnings.
The final instructed Kimi attempt spent its budget reading and searching,
without an observed draft or whole-proof check.

All eight agent processes reached their allotted deadline. Each complete
attempt, including preparation and session export, finished in about 140.4 to
140.5 seconds within the unchanged 150-second limit. The launcher reserves ten
seconds for collection by stopping model responses and tools earlier. The
largest sampled complete-attempt memory peak was about 556 MiB, below the
unchanged 768 MiB cap. Short sampling intervals can still miss brief peaks.

The eight complete attempts total 1,123.536 seconds. An earlier interrupted
attempt adds 150.066 seconds; keep that failure in the overall cost account.
Session exports and observed token categories were retained. Interrupted
response usage and actual account charges remain unknown. A reported zero
catalog price does not establish a free request.

See the [reviewed attempt summaries](local-results/summary.json) and
[interrupted collection](interrupted-collection.json). Only reviewed summaries
and submitted patches are public. Raw prompts, transcripts, drafts, task
archives, session exports and account data remain private.

## Preparation fixes and checks

Fr now gives arithmetic proof modules an explicit integer-division import.
The leading `prelude` header disables Lean's automatic full `Init` import.
New package entries preserve that header. Generated packages and direct proof
checks use one compiler worker. Both instruction settings had the same context;
the claim and resource limits stayed fixed.

The [probe summary](qualification-probes.json) preserves the earlier failures:
an input-format bug in patch validation, a completed workflow without enough
memory reserve, and rejected client tuning. Isolated import checks motivated
the generator change; they do not establish a general performance improvement.
Four fresh complete client checks then passed with the generated imports.

Freezing caught a moving Homebrew Python link. The controller now checks its
explicit Python version before native work. The first paid baseline attempt
then exposed a missing enforced finishing reserve. Its collector was stopped
at 150 seconds before process results and accounting were saved. That packet
was stopped permanently, including seven unstarted slots. The repaired
comparison used distinct `argument-repair-` identifiers and never reopened the
interrupted slot.

[Hosted qualification](https://github.com/e6qu/fun-refactor/actions/runs/41914871055)
accepted the complete reference and rejected invalid drafts and altered patches.
All four fresh local client/settings combinations passed a complete scripted
workflow before the new slots were frozen. It covered goal inspection, the
mapped type error, corrected proof, save, package build, patch replay and
accounting. Those controls received reference drafts solely to test the setup.
Live attempts received none. No timeout or resource limit was increased.

## Next work

Give each attempt its exact theorem target, a valid starting cursor and the
current goal. Teach the observed JSON shapes and assumption patterns with
small, checked examples. Exercise saving, package checking and patch delivery
as one complete workflow. Require scripted controls to cover an agent that
ignores its finishing deadline before another paid comparison.

A graph proposal must address a recorded navigation or goal error and be
compared against the same explicit command workflow. These attempts provide
no measurement of graph-guided proving. Keep the next prediction and success
rule fixed before calls; do not repeat attempts until one happens to pass.
