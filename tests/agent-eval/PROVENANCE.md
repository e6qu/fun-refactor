# Agent acceptance source and evidence

`strsim-0.11.1.crate` is the unmodified published source archive for rapidfuzz/strsim-rs 0.11.1.
The existing Cargo cache supplied these bytes. No source padding or injected bug modifies the benchmark basis.

- Upstream: https://github.com/rapidfuzz/strsim-rs
- Published archive: https://static.crates.io/crates/strsim/strsim-0.11.1.crate
- SHA-256: `7da8b5736845d9f2fcb837ea5d9e2628564b3b043a70948a3f0b778838c5fb4f`
- The archive's `.cargo_vcs_info.json` records commit `76c5a900e6e12cfc605eee5ab6e36300384c8682`.
- License: MIT; the archive contains the notice, also retained in [LICENSE.strsim](LICENSE.strsim).

This is the complete published crate snapshot, including source, tests, documentation and benchmarks.
It excludes repository files that upstream omitted from the release and contains no upstream Git history.
The archive is evaluation data; no fr library or release target compiles it into the product.
The evaluation extracts it into a temporary directory and invokes its own Rust tests.

The harness adds `.fr/checks.json` with one explicit upstream test command and initializes disposable Git metadata.
It records original tracked bytes and modes before agents start.
The two task prompts request a Unicode Sørensen–Dice fix and a normalized OSA API addition.
Independent oracles live in `tools/agent_eval/oracle.py` outside the agent's permitted inspection surface.

[The initial evidence manifest](results/2026-09-07/manifest.json) binds the retained prompts, instrumented transcripts, scores, patches and skill bundle by SHA-256.
It includes interrupted pilot transcripts and explains their exclusion from scored trials.
The results retain local paths for exact token auditing; those paths do not grant access to live projects.
Git attributes preserve evidence bytes and allow the context-only space lines that exported patches require.
Raw transcripts include excerpts of the MIT source and agent-authored changes.
Behavioral replay needs the archive and standard local tools. Token auditing separately needs the pinned tokenizer and vocabulary.

The separate [regex workspace evaluation](../../docs/agent-workspace-evaluation.md) pins a complete upstream workspace, a dependency lock and controlled rehearsal evidence.
Its source archive, licenses and lock live under `regex/`; it does not replace the strsim fixture or earlier transcripts.
