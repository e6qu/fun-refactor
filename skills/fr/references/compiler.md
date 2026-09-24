# Compiler evidence

For compiler investigations, review `checks --toolchain` and retain full execution output with
`--output-bytes 65536`. Declare relevant environment keys and external identity files in each check.
The toolchain identity also covers workspace Rust manifests, lockfiles, toolchain files and `.cargo` configuration.
Use the actual compiler path or explicitly declare the compiler selected by a launcher.

`fr_ir.compiler_evidence.CompilerEvidence.inspect(client, retained, check="compiler")` reads rustc JSON;
select `format="cargo-json"` for Cargo. It executes no checks. Read capture, disclosure and command
outcome separately. Complete diagnostics can describe a failed compilation. Follow exact source actions
only when needed. Keep expanded, external and invalid spans unresolved. Attach observations through
`page.attach(plan, client, step)`; failing checks remain failures. The caller vouches for retained output.
