# Declared Rust and Python explanation profile

Collected in [run 38099146071](https://github.com/e6qu/fun-refactor/actions/runs/38099146071)
after the expression-comment fix. The manifest binds the source commit, executable, downloaded
artifact and exact report bytes. Source bindings include native code and Python parser build inputs.

Eleven declared features use eighteen original and renamed/relocated cases, plus the compiler
and checked-source-correspondence collectors. All 26 corruption controls pass.

These finite cases do not establish complete language semantics, runtime dispatch,
agent efficiency, security properties or source-connected proofs.

Replay with `python3 -B tools/test-compiler-profile.py` under the local resource guard.
Full compiler/runtime collection belongs on GitHub runners.
