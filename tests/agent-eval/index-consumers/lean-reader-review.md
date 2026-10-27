# Why the repository benchmark answers changed

The benchmarks read two fixed copies of this project. A reader improvement can find code
that the earlier reader missed, even though those source files have not changed.
We preserve the original timing baselines and record the changed answers separately.

For example, in `simp only [lemma] at accepted ⊢`, `only` and `at` instruct the Lean
simplifier; they are not variables used by the program. Earlier parsing sometimes treated
them as expressions and continued into the next declaration. The repaired grammar keeps
these proof instructions together and recovers the later declarations.

Two GitHub runs built both readers independently and read the same source bytes:
[consumer snapshot review](https://github.com/e6qu/fun-refactor/actions/runs/40477575144) and
[indexing snapshot review](https://github.com/e6qu/fun-refactor/actions/runs/40478787425).
The exact reader commits, source hashes and every changed location are in
[the retained inventory](lean-reader-review.json).

The latest repair accepts inline `show … by …` proof terms that Lean accepts.
Both fixed copies retain exactly the previous reviewed facts, syntax results and 512 question sites.
The original timing baselines stay unchanged; current timings need the rebuilt executable.

| Fixed source copy | Before this reader repair | After this reader repair |
| --- | ---: | ---: |
| Consumer benchmark declarations and parameters | 98,147 | 98,231 |
| Consumer benchmark reference occurrences | 592,956 | 592,648 |
| Indexing benchmark declarations and parameters | 98,355 | 98,439 |
| Indexing benchmark reference occurrences | 594,282 | 593,974 |

The original baselines each contain six more references than the “before” column.
Those six were false variable reads of `_` in patterns such as `rcases value with _ | other`.
Their earlier correction and exact source locations remain checked separately.

Both copies have the same Lean files. The repair adds 84 declarations and parameters and
removes none: 18 functions and 62 parameters in `Workspace.lean`, plus one function and its
parameter in each of `Source.lean` and `Project.lean`. Thirteen existing declaration records
change their extent or enclosing declaration. Reference changes remove 484 records, add
176 records and change the target or classification details of 311 otherwise matched records.
A removed record paired with an added record at the same source location can be a change
from a plain name to a function call, rather than a lost occurrence.

For example, `change n ≤ limit` asks Lean to restate the current goal.
The reader previously ended the proof after `change n` and misread the remaining comparison.
It now keeps the comparison inside that proof step. This removes the parser errors in
`Project.lean` and `Patch.lean` and two false variable reads of the tactic names `rcases` and `rw`.
No declaration disappears, and no previously clean file gains a parser error.

The checks hash all current Lean declarations, parameters and references using source
locations instead of temporary numeric IDs. They require the reviewed digest, all 59 Lean
file hashes and the exact counts. The unchanged smaller workloads still require their
original complete answers. Each indexing repetition must produce the same full answer
digest. Each consumer answer must agree with a separate linear search of the current index.

The consumer test previously selected every nth declaration. Recovering missing declarations
silently changed 445 of its 512 questions. It now selects the
[same 512 recorded source definitions](repository-queries.json), failing if any is missing or
ambiguous. The old timing baseline did not retain its chosen locations, so we do not claim
to reconstruct that historical selection or an identical-question speed improvement.

This is a check of the recorded syntax facts, not proof that the reader understands every
Lean construct. Generic proof instructions can still appear as identifier references, and
some Lean source forms still contain parser errors. These measurements include indexing,
answer construction and language-support changes. They do not establish a runtime speedup
for user programs, agent savings or equivalence between a source program and its Lean model.
