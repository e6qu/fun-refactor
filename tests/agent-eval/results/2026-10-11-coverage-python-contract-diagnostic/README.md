# Failed first expanded contract collection

[Hosted run](https://github.com/e6qu/fun-refactor/actions/runs/38124026322) collected all
252 cases and rejected ten observations. This directory preserves that output unchanged.
It is diagnostic evidence and does not close a roadmap obligation.

Four failures expected a generic call-syntax cutoff for argument unpacking. The analyzer
reports the more precise `argument-unpacking-unchecked` boundary.

Four failures assumed every witness trace ended at its sink. Imported sink summaries
append caller records after the sink. The corrected audit requires one exact sink endpoint,
checks every occurrence against its file, and permits only AST-backed summary calls afterward.
Corruption tests reject altered endpoints, suffix roles and non-call suffix locations.

Two failures expected no witness when an import was unresolved. The analyzer retains
argument origins through an unknown call and marks the report incomplete. The corrected
example records this difference from Python's definition-time import exception explicitly.
No runtime outcome, source program or analyzer answer was rewritten to hide the discrepancy.

The successful replacement collection must be retained separately. The manifest records
the original source commit, artifact identity and exact result digest.
