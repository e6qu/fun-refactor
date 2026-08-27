//! Every method of the browser `Workspace` says whose files it is reading.

use std::path::Path;

#[test]
fn every_workspace_method_activates_its_own_files() {
    let source = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/wasm.rs"))
        .expect("reading src/wasm.rs");

    // Every block, not the first.
    let mut missing = Vec::new();
    let mut checked = 0;
    let mut blocks = 0;
    let mut rest = source.as_str();

    while let Some(start) = rest.find("impl Workspace {") {
        blocks += 1;
        // Only the impl block: `version()` is a free function with no workspace to enter.
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
        "only found {checked} methods: the parse of src/wasm.rs is wrong, not the file"
    );
    assert!(
        missing.is_empty(),
        "these must call `self.enter()` on their first line, so they read their own \
         workspace's bytes instead of the last one loaded:\n  {}",
        missing.join("\n  ")
    );
}
