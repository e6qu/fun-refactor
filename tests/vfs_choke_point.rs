//! The analysis reads source through `crate::vfs` and nowhere else.

use std::path::Path;

/// Files that are allowed to touch the filesystem directly, and why.
fn is_exempt(path: &Path) -> bool {
    let name = path.to_string_lossy().replace('\\', "/");
    // vfs.rs *is* the choke point.
    name.ends_with("src/vfs.rs") || name.ends_with("src/cache.rs") || name.ends_with("src/scan.rs")
}

/// Text inside `#[cfg(test)]` is a test fixture building a temporary directory, which
/// is a filesystem operation by nature and never runs in a browser.
fn without_test_modules(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut lines = source.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("#[cfg(test)]") {
            // Skip to the end of the module: the closing brace at the same indent.
            let indent = line.len() - line.trim_start().len();
            let closing = format!("{}}}", " ".repeat(indent));
            for inner in lines.by_ref() {
                if inner == closing {
                    break;
                }
            }
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn rust_sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("reading src/") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

#[test]
fn source_is_read_only_through_the_vfs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_sources(&root, &mut files);
    assert!(!files.is_empty(), "found no Rust sources under src/");

    // `create_dir_all` and `rename` are excluded deliberately: they are write-side filesystem
    // mechanics that `edit.rs` performs only on the terminal path.
    let banned = [
        ("std::fs::read_to_string", "crate::vfs::read_to_string"),
        ("std::fs::read(", "crate::vfs::read_to_string"),
        ("std::fs::write", "crate::vfs::write"),
        ("std::fs::read_dir", "crate::vfs::read_dir"),
        (".exists()", "crate::vfs::exists"),
    ];

    let mut offences = Vec::new();
    for file in &files {
        if is_exempt(file) {
            continue;
        }
        let source =
            without_test_modules(&std::fs::read_to_string(file).expect("reading a source"));
        for (line_number, line) in source.lines().enumerate() {
            for (needle, instead) in banned {
                if line.contains(needle) {
                    offences.push(format!(
                        "{}:{}: `{}`: use {} so this works in the browser too\n    {}",
                        file.strip_prefix(&root).unwrap_or(file).display(),
                        line_number + 1,
                        needle,
                        instead,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        offences.is_empty(),
        "the analysis must reach source through src/vfs.rs:\n\n{}\n",
        offences.join("\n")
    );
}
