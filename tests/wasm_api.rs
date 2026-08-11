//! Every method of the browser `Workspace` says whose files it is reading.
//!
//! `tests/wasm_native.rs` now drives the API itself, and `cargo check --features wasm`
//! type-checks it, neither of which was true when this file was written. What it
//! still cannot do is prove the invariant holds for *every* method: `enter()` is only
//! observable when two workspaces exist at once, and a test that calls one method at a
//! time would pass while a method that forgot it stayed broken.
//!
//! So this reads the source. Each method must call `self.enter()` before touching
//! source.
//!
//! Without it the method reads whichever workspace was created most recently. Two
//! repositories open in one page is enough to trigger it, and the failure is silent,
//! spans measured against one file's bytes, applied to another's. That is how the
//! playground came to report a rewrite as unavailable at a position where applying it
//! worked: the listing re-read the file and got a different workspace's text.
//!
//! Reading the source is a poor substitute for a type, and it is exhaustive, which the
//! tests that call the methods are not.

use std::path::Path;

#[test]
fn every_workspace_method_activates_its_own_files() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/wasm.rs"))
        .expect("reading src/wasm.rs");

    // Every block, not the first. The methods live in one `impl` and the constructors
    // in another, and a scan that stopped at the first `}` found four methods, said
    // nothing, and passed.
    let mut missing = Vec::new();
    let mut checked = 0;
    let mut blocks = 0;
    let mut rest = source.as_str();

    while let Some(start) = rest.find("impl Workspace {") {
        blocks += 1;
        // Only the impl block: `version()` is a free function with no workspace to
        // enter. The block ends at the first `}` in the first column.
        let lines: Vec<&str> = rest[start..]
            .lines()
            .skip(1)
            .take_while(|line| *line != "}")
            .collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            // The two constructors install the handle they have just built and take no
            // `self`, so there is nothing for them to enter.
            if !trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub fn new(")
                || trimmed.starts_with("pub fn load(")
            {
                continue;
            }
            checked += 1;
            let follows = lines.get(i + 1).map(|l| l.trim()).unwrap_or("");
            if follows != "self.enter();" {
                missing.push(trimmed.to_string());
            }
        }
        rest = &rest[start + "impl Workspace {".len()..];
    }

    assert!(blocks >= 1, "src/wasm.rs no longer has an `impl Workspace`");

    assert!(
        checked > 20,
        "only found {checked} methods — the parse of src/wasm.rs is wrong, not the file"
    );
    assert!(
        missing.is_empty(),
        "these must call `self.enter()` on their first line, so they read their own \
         workspace's bytes instead of the last one loaded:\n  {}",
        missing.join("\n  ")
    );
}
