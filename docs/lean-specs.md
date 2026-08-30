# Specs in Lean

A plan, not a build. Nothing here exists yet.

`fr` reads Lean as of 0.6.0. This is what it would take to make Lean the place this
project writes down what its code is supposed to do, and to keep the two from drifting
apart. It also says which parts of that idea are worth doing and which are not.

## The one idea the design rests on

Writing a proof is a search. Checking one is a decision. Lean draws that line for us, and
it is the same line that separates a tool from an agent.

So: **`fr` owns everything decidable and the agent owns the search.** `fr` extracts a
spec's shape from the code, tells you where a spec and its code have drifted, counts what
is unproved, generates code from a spec, and runs `lake` to accept or reject an answer. It
never decides that a proof is good. An agent writes the proof and hands it back, and
`lake` is the only thing that says yes.

That gives a work list a tool can compute and an answer a tool can check, with the
judgement in between belonging to whoever is better at judgement.

## Four things worth arguing about first

### "Keep Lean specs in sync with the code" claims more than anyone can deliver

Sync means three different things and only one of them is decidable.

1. **The shapes agree.** The Lean spec's signature still matches the function's. `fr` can
   decide this, cheaply, forever. This is what `tests/docs_cli.rs` already does for
   prose, and it is the whole of what a tool can promise.
2. **The behaviours agree on the cases we ran.** Generate the target-language code from
   the Lean, run both, diff. `fr` already does exactly this for 87 conformance cells
   across six languages. Real, checkable, and not a proof.
3. **The implementation refines the spec.** A theorem, and nothing generates it. Proving
   a hand-written Rust function meets a Lean spec needs a formal semantics of Rust, which
   nobody has. This one is unavailable at any price and the plan should not imply it.

Everything below promises (1) and (2). Anywhere the word "verified" would suggest (3),
the word is wrong.

### A `theorem` generates nothing

A Lean `def` has computational content and can become Rust. A `theorem` is a proof that a
proposition holds, and its content is erased. So "generate the implementation from the
spec" only works where the spec *is* the implementation, written in Lean.

That is not a limitation to work around, it is the shape of the feature: **a spec written
as a `def` is executable and generates code; a spec written as a `theorem` is a claim
about that code and generates a test obligation.** Both are useful. Conflating them
produces a feature that appears to work and quietly emits stubs.

### There is no Lean-to-Rust extraction, and this project does not need one

Lean 4 compiles to C. Nothing extracts it to Rust. Three ways out:

- **Link the C.** Real Lean semantics, and a Lean toolchain becomes a build dependency of
  everything. Against the grain of a tool whose build needs a C compiler and nothing else.
- **Hand-write the Rust and prove correspondence.** See (3) above.
- **Generate the Rust from a restricted Lean, with `fr`.** `fr` is already a transpiler
  with a canonical IR and a harness that proves six languages print the same transcript.
  Lean as a source is one more reader.

The third is the only one that fits, and it costs a Lean reader rather than a research
programme.

### Proving the refactorings correct is not the place to start

The tempting target is "prove `fr rename` preserves meaning". It needs a formal semantics
for nineteen tree-sitter grammars. It will not happen.

The IR is the opposite: 9 items, 26 statements, 34 expressions, self-contained, and the
place the real risk lives. A wrong lowering is silent, and the ledger that went from 1,756
carried constructs to zero is a count of shapes that cross, not a claim that they cross
correctly. **Specify the IR, prove things about the writers, and leave the grammars
alone.**

## What Lean is worth here, in order

### Tier 1: the IR has a semantics

Write `Ir.lean`: the item, statement and expression types, and an evaluator for the subset
that has one. Then the properties worth having:

- Reading a writer's output returns the term you started with, for every construct the
  round-trip suite covers.
- Bracketing preserves meaning: the operator-precedence rule that four separate defects
  came from.
- Division and remainder agree with each target's own rounding, which is B633 and the
  `Math.trunc` family written down instead of remembered.

This is the highest-value tier because it is where `fr` is actually most likely to be
wrong, and it needs no code generation at all.

### Tier 2: Lean is a translate target and a source

The writer first: `Record` becomes a `structure`, `Sum` an `inductive`, `Function` a
`def`, `Newtype` an `abbrev`. It refuses what it should: mutation outside a monad,
recursion it cannot show terminates, anything reaching a runtime.

Then the reader, over the same restricted subset. That is what makes generation possible,
and it is the only new machinery the whole plan requires.

### Tier 3: the kernel pattern

Mark a module `@[fr.kernel]`. `fr` generates the target-language implementation from it
and adds a conformance cell that runs both and diffs the transcript. Not a proof of
correspondence, and a much stronger claim than a comment saying the two agree.

This is where "write the kernel in Lean" becomes a thing a person can actually do, and it
reuses a harness that already exists.

## The commands

```
fr spec extract <path>            derive a Lean skeleton from code: types, signatures,
                                  an anchor comment, and `sorry` where a claim goes
fr spec check                     the drift report: shapes that moved, specs whose
                                  symbol is gone, code a policy says wants a spec,
                                  and the count of unproved obligations
fr spec sync <path>               re-derive the signature half of a spec in place,
                                  leaving every proof alone
fr translate Foo.lean rust        generate from a spec that is a `def`
fr spec verify                    run `lake`, and say what failed and where
```

`fr spec extract` writes an anchor the rest depends on:

```lean
-- fr:spec src/refactor/rename.rs::plan @ 8f2c1a9e
def plan (index : Index) (symbol : SymbolId) (newName : String) :
    Except Refusal Plan := sorry
```

The hash is the function's own bytes. When it changes, `fr spec check` says which spec
went stale and why. That is the `docs_cli.rs` trick pointed at code instead of prose, and
it is the whole sync story: a tool that notices, not a tool that promises.

## The lifecycle, which is the part that usually goes wrong

Generation that only writes stubs is a demo. The three later moves are the feature.

- **Generate.** The output carries `fr:from-spec` anchors around each generated region.
- **Regenerate.** A region nobody touched gets replaced. A region a person edited stops
  the run and names the file and line. `fr` already refuses rather than overwrite, and
  this is that discipline applied to a second author.
- **Reverse.** A signature changed in the code, and `fr spec sync` carries it back to the
  Lean and leaves the proofs standing, so the next `lake` run says which ones broke.

The proofs breaking is the point. A spec that survives a change to the thing it specifies
was not specifying much.

## Where the agent goes

`fr` computes the work list and owns the oracle. The agent does the search.

- **`lean-prover`** — takes one `sorry` and its context, tries to discharge it, and
  offers a patch. The loop ends when `lake build` accepts, and `fr` runs `lake`, not the
  agent. Parallel over independent obligations, since each is its own question.
- **`spec-author`** — takes a drift report and writes or repairs the claims. This is the
  judgement-heavy end: which properties are worth stating at all.

Both are advisory. Nothing reaches a file without `lake` having accepted it, which is why
the non-determinism upstream is safe.

## The ratchet

`SPEC-DEBT`, next to `PROSE-DEBT`, holding `sorry` at a number that only falls. An
unproved obligation is debt with a name, which is better than an intention.

## Order, and the one to build first

Tier 2's writer, because everything else waits on it and it extends machinery that
already works. Then the anchor and `fr spec check`, which is deterministic, cheap, and
useful on its own before a single proof exists. Then the reader, then the kernel cell.

Tier 1 can start any time and is the most valuable thing here. It is also the easiest to
put off, being the only part with no visible output.

## What to leave alone

Proving the refactorings. Proving a hand-written implementation refines its spec. Any use
of the word "verified" for a correspondence that a conformance run established rather than
a proof.
