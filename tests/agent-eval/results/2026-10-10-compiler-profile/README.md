# Declared Rust and Python explanation profile

Collected in [run 38076799667](https://github.com/e6qu/fun-refactor/actions/runs/38076799667) at `10f6b4712c8b96c31e018885350c398f4f7a0427`.
The manifest binds the downloaded archive, executable and exact report bytes.
The report binds the native implementation, Python parser build inputs, SDK and collectors.

Eleven declared features use eighteen original and renamed/relocated cases, plus the
existing compiler and checked-source-correspondence collectors. Commands, input source,
source locations, unsupported results, compiler/runtime versions and outcomes remain visible.

These finite cases do not establish complete language semantics, runtime dispatch,
agent efficiency, security properties or source-connected proofs.

Replay with `python3 -B tools/test-compiler-profile.py`.
Full compiler/runtime collection belongs on GitHub; local work stays under the resource guard.
