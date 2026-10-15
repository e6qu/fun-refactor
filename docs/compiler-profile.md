# What a code explanation really establishes

Finding a function, compiling it and running a test answer different questions. A parsed function may still fail to compile.
For example, `fr` can locate a Rust expression while the compiler reports a type error at that expression. This guide explains how to read each result without treating it as a stronger guarantee.

A parser recognizes written syntax. A compiler also checks whether code satisfies its language's build rules.
CPython is the standard Python interpreter used for these execution checks.
An oracle is an independent reference check, such as executing the example with a known input.
A profile here is a declared set of features to check, not a CPU performance profile.
For an explanation involving a type error, retain the compiler configuration and error location alongside the parsed source.

## Commands, results and limits

`fr` reports several kinds of evidence. A function name and source location can come from the
syntax index. A compiler diagnostic comes from a declared command. A runtime result comes from
executing a particular program with particular inputs. These facts answer different questions.

For example, the Rust fixture parses in both configurations. Its ordinary configuration compiles
and passes six runtime cases. Adding `--cfg fr_strict` produces an `E0308` type error at the exact
`value` expression. The report preserves both syntax acceptance and compiler rejection.

The [profile collector](../tools/compiler-profile.py) combines the existing compiler and Python
source-correspondence collectors with eighteen focused cases. Nine cases repeat after renaming
functions and moving the source into a nested directory. Their source positions change; their
declared answers and limitations must remain the same.

## Declared features

| Feature | Evidence the profile checks | Boundary |
| --- | --- | --- |
| Rust compiler configuration | Default acceptance, strict `E0308` rejection, exact primary span, Cargo configuration and six runtime inputs | Parsing does not establish compiler acceptance |
| Rust repeated calls | Two calls on one line retain distinct locations, including Unicode before the second call | Indexed relationships are candidates for runtime dispatch |
| Rust shadowing (a nearer declaration hiding an earlier name) | A local closure shadows a same-named function; execution returns the closure's value | The call stays unresolved; Rust value flow is refused |
| Python repeated calls | CPython AST positions agree with call-name and full-expression locations; runtime checks use three inputs | Expression origins do not authorize source mutation |
| Python duplicate declarations | Both declarations have exact name locations; runtime uses the last definition | Selecting one ambiguous lowered body explicitly refuses |
| Python modulo lowering (converting source into the structured code model) | Runtime agrees on zero, positive and negative inputs | Normalized expressions retain absent the connection between source and its model |
| Python local shadowing | A later local assignment causes `UnboundLocalError` before the configured source can run | Analysis remains incomplete and produces no source-to-sink witness |
| Python exception handler | Runtime catches division by zero and returns seven | The analyzer reports an incomplete result |
| Python scalar helper | A configured source value reaches a sink through a helper; every trace location matches an independent AST node | A possible-flow claim on a finite scalar case |
| Python overwrite | Assignment replaces the source value before the sink; runtime sees zero and analysis reports no witness | A finite scalar case, not a general absence proof |
| Python checked origins | Existing syntax-tree and execution checks complete a task; a source change invalidates it and failed checks stay failed | Checked task evidence remains tied to its inputs |

Offsets are half-open UTF-8 (the text encoding used to count source bytes) source byte ranges. Line and column positions are one-based and count Unicode
characters. The independent Python oracle uses CPython's byte-based AST positions and converts
them to that convention. Rust fixture call positions use separately counted source tokens;
`rustc` supplies compiler diagnostic positions and executes the runtime drivers.

Reviewing the first collection exposed a bug: selecting the second of two same-named Python
functions returned the first body's model with absent origins. Absent origins did not make that
selection correct. Semantic queries and semantic edit preparation now require uniqueness in both
the source index and the lowered model, including containing declarations. Regression tests cover
duplicates, decorated definitions, methods and classes; distinct qualified methods remain usable.
Static methods without receiver metadata require one callable of that name in the file.
When selection refuses, inspect the file or request bounded source instead.

## Retained result

The [retained report](../tests/agent-eval/results/2026-10-11-coverage-compiler-profile/README.md) passes all
eleven declared features, eighteen cases and 26 tests that reject altered records. Nine cases rename and relocate
the original source. One Rust configuration deliberately compiles unsuccessfully despite parsing.
These results close A.compiler-profile only for the declared features and outcomes.
The hosted job, including lint and all 88 authoring tests, completed in 13 minutes 55 seconds.

## Reproduction and review

Run the `compiler-profile` group in the existing **Refresh refinement evidence** workflow.
It builds the CLI on GitHub and has a fifteen-minute job limit. Do not run this collection on a
shared workstation. The collector retains partial cases before validating the complete profile.

The report retains source and configuration, exact commands and responses, compiler/runtime
versions, executable identities and recorded input fingerprints. Bindings include the patched Python parser,
its build script and release-normalized Cargo manifests. Both reused collectors must use the same
CLI executable and repository revision as the new cases. The compiler collector also tests
changed source, environment, external identity and build configuration, plus truncated output.

The native compiler-evidence gate runs the profile's tests that reject altered records. They reject missing or
duplicate cases, narrowed feature declarations, wrong coordinates, invented call targets, lost
unsupported boundaries, altered runtime results and mismatched executable identities.

## What remains open

This is a declared feature profile, not complete Rust or Python semantics. It contains prescribed
programs and the selected runtime inputs. Renamed examples test name and path independence; they do
not substitute for independently authored programs or unfamiliar repositories.

The report does not establish agent quality, token savings, runtime dispatch, security properties,
arbitrary value flow or proofs connected to the actual source. Rust compiler results do not certify Python
analysis. A complete diagnostic page can describe a failed compilation; an incomplete analysis
cannot prove that a value never reaches a use.

The next analysis step is [B.contract](roadmap-status.md#bcontract-specify-reusable-python-behavior):
state reusable Python rules, compare them with independent runtime examples, and retain explicit
unsupported constructs. Repository-scale and requirements to show less total agent work remain separate.
