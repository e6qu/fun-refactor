//! Every method of the browser `Workspace` says whose files it is reading.
//!
//! `src/wasm.rs` is compiled only for `wasm32`, so nothing in a normal `cargo test`
//! type-checks it and no test can call it. It is still the whole public surface of the
//! playground, and it has one invariant that cannot be expressed in the type system:
//! each method must call `self.enter()` before touching source.
//!
//! Without it the method reads whichever workspace was created most recently. Two
//! repositories open in one page is enough to trigger it, and the failure is silent —
//! spans measured against one file's bytes, applied to another's. That is how the
//! playground came to report a rewrite as unavailable at a position where applying it
//! worked: the listing re-read the file and got a different workspace's text.
//!
//! Reading the source is a poor substitute for a type, but it is the only check
//! available for a file this crate cannot compile on the host.

use std::path::Path;

#[test]
fn every_workspace_method_activates_its_own_files() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/wasm.rs"))
        .expect("reading src/wasm.rs");

    let start = source
        .find("impl Workspace {")
        .expect("src/wasm.rs no longer has an `impl Workspace`");
    // Only the impl block: `version()` is a free function with no workspace to enter.
    let lines: Vec<&str> = source[start..]
        .lines()
        .skip(1)
        .take_while(|line| *line != "}")
        .collect();
    let mut missing = Vec::new();
    let mut checked = 0;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        // The constructor installs the handle it has just built, and takes no `self`.
        if !trimmed.starts_with("pub fn ") || trimmed.starts_with("pub fn new(") {
            continue;
        }
        checked += 1;
        let follows = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
        if follows != "self.enter();" {
            missing.push(trimmed.to_string());
        }
    }

    assert!(
        checked > 20,
        "only found {checked} methods — the parse of src/wasm.rs is wrong, not the file"
    );
    assert!(
        missing.is_empty(),
        "these must call `self.enter()` on their first line, so they read their own \
         workspace's bytes rather than the last one loaded:\n  {}",
        missing.join("\n  ")
    );
}
