# Compare upstream Lean memory before changing the product pin

The import diagnostic and immediate-purge experiment left native proof admission
blocked. Further allocator tuning has no measured basis. Instead, compare the
original Lean 4.28.0 release with stable 4.36.0, published November 16, 2026.
This is a new toolchain diagnostic, not a continuation or retry of either prior
experiment. Historical pins, results and study acceptance rules remain unchanged.

## Hypothesis

Upstream import and runtime changes may reduce the complete checker footprint.
The [4.35 release notes](https://lean-lang.org/doc/reference/latest/releases/v4.35.0/)
describe smaller mimalloc objects (#14948), shared modifier parsers (#15161) and
deferred premise-selection indexes (#15159). The
[4.36 release](https://lean-lang.org/doc/reference/latest/releases/v4.36.0/)
includes further performance fixes. A whole-release comparison cannot attribute
any reduction to one of those changes; it can establish whether a migration is
worth qualifying. Library growth or incompatibilities may instead rule it out.

## Fixed inputs and schedule

[The protocol](toolchain-protocol.json) pins both official archive hashes on Linux
and ARM macOS. Use qualification run 44542024683's original seed and independently
reconstructed reference proof. Preserve every source file, including the theorem,
program, `import Std`, package configuration and original `lean-toolchain` file.
Invoke each release's absolute `bin/lake -d specs build --wfail`, bypassing Elan
selection deliberately for this diagnostic. The changed variable is the complete
Lean distribution, including its standard library, Lake and compiler. Record
executable hashes and version output for both. Neither condition uses old build
outputs, modified imports, a different proof, extra axioms or disabled checks.

Remove ambient Lean and mimalloc overrides and give each condition one Lean worker.
Run baseline then candidate. Run a second pair, candidate then baseline, only if
both first checks pass. Run candidate unfinished and wrong-proof controls only
if every started candidate reference check passes. Reuse the existing protected
proof-region controls and require their specific proof diagnostics. Keep every
failure and unstarted case. Never retry a stopped case. Each case has a fresh
workspace; OS caches are not reset. Both platforms run independently on GitHub.

Use the unchanged 60-second, 768 MiB sampled aggregate RSS, 25 sampled CPU-second,
64 MiB growth and 128 KiB output limits. Retain the original 100 ms process sampler,
all Lake/Lean descendants, stdout, stderr and resource verdicts. Tool downloads
and version probes are setup, outside these checker-only measurements. No model
calls, native admission retries or reserved validation projects are involved.

## Frozen decision

Require all four reference checks and both intended negative rejections. The
larger candidate peak must be at least 10% below the smaller baseline peak and
at most 436 MiB. The latter leaves 204 MiB for the client/controllers inside the
640 MiB budget after the existing 128 MiB admission reserve. This is a screening
budget based on the original native stop, not a substitute for measuring clients.
Publish wall time and CPU alongside memory. A refusal or incomplete pair cannot
support adoption; a small, inconsistent or insufficient reduction is discarded.

A useful result only justifies qualifying a toolchain migration against all
current proof workflows and independent examples. It does not authorize silently
changing frozen study pins. Admission remains blocked until that integration and
all 24 complete native rehearsals pass under unchanged limits and reserve.
Local work also remains subject to the 64 GiB free-disk floor.
