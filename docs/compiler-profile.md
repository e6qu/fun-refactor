# What Rust and Python explanations establish

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
| Rust shadowing | A local closure shadows a same-named function; execution returns the closure's value | The call stays unresolved; Rust value flow is refused |
| Python repeated calls | CPython AST positions agree with call-name and full-expression locations; runtime checks use three inputs | Expression origins do not authorize source mutation |
| Python duplicate declarations | Both declarations have exact name locations; runtime uses the last definition | Selecting one ambiguous lowered body explicitly refuses |
| Python modulo lowering | Runtime agrees on zero, positive and negative inputs | Normalized expressions retain absent source correspondence |
| Python local shadowing | A later local assignment causes `UnboundLocalError` before the configured source can run | Analysis remains incomplete and produces no source-to-sink witness |
| Python exception handler | Runtime catches division by zero and returns seven | The analyzer reports an incomplete result |
| Python scalar helper | A configured source value reaches a sink through a helper; every trace location matches an independent AST node | A possible-flow claim on a finite scalar case |
| Python overwrite | Assignment replaces the source value before the sink; runtime sees zero and analysis reports no witness | A finite scalar case, not a general absence proof |
| Python checked origins | Existing AST/runtime checks complete a task; a source change invalidates it and failed checks stay failed | Checked task evidence remains tied to its inputs |

Offsets are half-open UTF-8 byte spans. Line and column positions are one-based and count Unicode
characters. The independent Python oracle uses CPython's byte-based AST positions and converts
them to that convention. Rust fixture call positions use separately counted source tokens;
`rustc` supplies compiler diagnostic positions and executes the runtime drivers.

Reviewing the first collection exposed a bug: selecting the second of two same-named Python
functions returned the first body's model with absent origins. Absent origins did not make that
selection correct. Semantic queries and semantic edit preparation now require uniqueness in both
the source index and the lowered model, including containing declarations. Regression tests cover
duplicates, decorated definitions, methods and classes; distinct qualified methods remain usable.
When selection refuses, inspect the file or request bounded source instead.

## Reproduction and review

Run the `compiler-profile` group in the existing **Refresh refinement evidence** workflow.
It builds the CLI on GitHub and has a fifteen-minute job limit. Do not run this collection on a
shared workstation. The collector retains partial cases before validating the complete profile.

The report retains source and configuration, exact commands and responses, compiler/runtime
versions, executable identities and source bindings. Bindings include the patched Python parser,
its build script and release-normalized Cargo manifests. Both reused collectors must use the same
CLI executable and repository revision as the new cases. The compiler collector also tests
changed source, environment, external identity and build configuration, plus truncated output.

The native compiler-evidence gate runs the profile's corruption tests. They reject missing or
duplicate cases, narrowed feature declarations, wrong coordinates, invented call targets, lost
unsupported boundaries, altered runtime results and mismatched executable identities.

## What remains open

This is a declared feature profile, not complete Rust or Python semantics. It contains prescribed
programs and finite runtime inputs. Renamed examples test name and path independence; they do
not substitute for independently authored programs or unfamiliar repositories.

The report does not establish agent quality, token savings, runtime dispatch, security properties,
arbitrary value flow or source-connected proofs. Rust compiler results do not certify Python
analysis. A complete diagnostic page can describe a failed compilation; an incomplete analysis
cannot prove that a value never reaches a use.

The next analysis step is [B.contract](roadmap-status.md#bcontract-specify-reusable-python-behavior):
state reusable Python rules, compare them with independent runtime examples, and retain explicit
unsupported constructs. Repository-scale and agent-efficiency requirements remain separate.
