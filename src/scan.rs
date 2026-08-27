//! Workspace discovery: find the files we can act on.

use crate::lang::Language;
use anyhow::Result;
use ignore::WalkBuilder;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A source file discovered in a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

/// Options controlling workspace discovery.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Respect .gitignore and friends.
    pub respect_ignore: bool,
    /// Restrict the scan to these languages; empty means all.
    pub languages: Vec<Language>,
    /// Skip files larger than this many bytes.
    pub max_file_bytes: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            respect_ignore: true,
            languages: Vec::new(),
            // Tree-sitter parse cost grows with file size; multi-megabyte files are
            // almost always vendored or generated. Skipping them is reported. It is not silent.
            max_file_bytes: 4 * 1024 * 1024,
        }
    }
}

/// Files found by a scan, plus what was deliberately skipped.
#[derive(Debug, Default)]
pub struct ScanResult {
    pub files: Vec<SourceFile>,
    /// Paths skipped for exceeding `max_file_bytes`, with their size.
    pub skipped_too_large: Vec<(PathBuf, u64)>,
    /// Symlinks that look like source files, with the reason each was skipped.
    ///
    /// The walker does not follow links, so these used to vanish without a word. A
    /// reader comparing the listing against `ls` saw files the tool never mentioned.
    /// The real file is scanned where it lives; the link is reported here.
    pub skipped_symlinks: Vec<(PathBuf, String)>,
    /// Files in no language this tool reads, counted by extension.
    ///
    /// Naming each one would bury the listing under lock files and images, and
    /// saying nothing let a reader believe the whole tree was read. A repository
    /// of `.sql` and one `.py` answered "1 file(s)" and left the rest to be
    /// guessed at. Counted here, named by extension, reported in one line.
    pub unsupported: BTreeMap<String, usize>,
}

/// Walk `root` and collect source files in a language we support.
pub fn scan(root: &Path, options: &ScanOptions) -> Result<ScanResult> {
    let mut result = ScanResult::default();

    let walker = WalkBuilder::new(root)
        .standard_filters(options.respect_ignore)
        .hidden(options.respect_ignore)
        .git_ignore(options.respect_ignore)
        // Honour .gitignore even outside a git repository: refactoring targets are
        // often worktrees, exports or vendored copies with no .git directory.
        .require_git(false)
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();
        if entry.path_is_symlink() && crate::lang::detect(path).is_some() {
            let reason = match std::fs::read_link(path) {
                Ok(target) => format!("symlink to {}", target.display()),
                Err(e) => format!("symlink whose target cannot be read: {e}"),
            };
            result.skipped_symlinks.push((path.to_path_buf(), reason));
            continue;
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Some(language) = crate::lang::detect(path) else {
            let key = match path.extension().and_then(|e| e.to_str()) {
                Some(extension) => format!(".{}", extension.to_ascii_lowercase()),
                None => "no extension".to_string(),
            };
            *result.unsupported.entry(key).or_default() += 1;
            continue;
        };
        if !options.languages.is_empty() && !options.languages.contains(&language) {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if size > options.max_file_bytes {
            result.skipped_too_large.push((path.to_path_buf(), size));
            continue;
        }
        result.files.push(SourceFile {
            path: path.to_path_buf(),
            language,
        });
    }

    result.files.sort_by(|a, b| a.path.cmp(&b.path));
    result.skipped_too_large.sort();
    result.skipped_symlinks.sort();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn workspace() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("src/app.py"), "def f(): pass\n").unwrap();
        fs::write(root.join("README.md"), "# hi\n").unwrap();
        fs::write(root.join("notes.bin"), "\x00\x01").unwrap();
        fs::write(root.join("target/generated.rs"), "fn gen() {}\n").unwrap();
        fs::write(root.join(".gitignore"), "target/\n").unwrap();
        tmp
    }

    #[test]
    fn finds_supported_files_and_skips_unknown_extensions() {
        let tmp = workspace();
        let result = scan(tmp.path(), &ScanOptions::default()).unwrap();
        let names: Vec<_> = result
            .files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"main.rs".to_string()));
        assert!(names.contains(&"app.py".to_string()));
        assert!(names.contains(&"README.md".to_string()));
        assert!(!names.contains(&"notes.bin".to_string()));
    }

    #[test]
    fn files_in_no_language_are_counted_by_kind() {
        let tmp = workspace();
        fs::write(tmp.path().join("schema.sql"), "select 1;\n").unwrap();
        fs::write(tmp.path().join("data.toml"), "a = 1\n").unwrap();
        fs::write(tmp.path().join("more.toml"), "b = 2\n").unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let result = scan(tmp.path(), &ScanOptions::default()).unwrap();
        assert_eq!(result.unsupported.get(".sql"), Some(&1));
        assert_eq!(
            result.unsupported.get(".toml"),
            Some(&2),
            "a kind is counted, and not listed once per file."
        );
        assert_eq!(result.unsupported.get("no extension"), Some(&1));
        assert!(
            !result.unsupported.contains_key(".rs"),
            "a language this reads is not passed over"
        );
    }

    #[test]
    fn a_language_filter_is_not_a_file_this_cannot_read() {
        let tmp = workspace();
        let opts = ScanOptions {
            languages: vec![Language::Rust],
            ..Default::default()
        };
        let result = scan(tmp.path(), &opts).unwrap();
        assert!(
            !result.unsupported.contains_key(".py"),
            "the reader asked for Rust. Python was not passed over for want of support."
        );
    }

    #[test]
    fn respects_gitignore_by_default() {
        let tmp = workspace();
        let result = scan(tmp.path(), &ScanOptions::default()).unwrap();
        assert!(
            !result
                .files
                .iter()
                .any(|f| f.path.ends_with("generated.rs")),
            "ignored directory should not be scanned"
        );

        let opts = ScanOptions {
            respect_ignore: false,
            ..Default::default()
        };
        let all = scan(tmp.path(), &opts).unwrap();
        assert!(all.files.iter().any(|f| f.path.ends_with("generated.rs")));
    }

    #[test]
    fn language_filter_narrows_results() {
        let tmp = workspace();
        let opts = ScanOptions {
            languages: vec![Language::Rust],
            ..Default::default()
        };
        let result = scan(tmp.path(), &opts).unwrap();
        assert!(!result.files.is_empty());
        assert!(result.files.iter().all(|f| f.language == Language::Rust));
    }

    #[test]
    fn oversized_files_are_skipped_and_reported() {
        let tmp = workspace();
        let opts = ScanOptions {
            max_file_bytes: 5,
            ..Default::default()
        };
        let result = scan(tmp.path(), &opts).unwrap();
        // Skipping must be visible to the caller, never silent.
        assert!(!result.skipped_too_large.is_empty());
        assert!(result
            .skipped_too_large
            .iter()
            .any(|(p, _)| p.ends_with("main.rs")));
    }

    #[test]
    #[cfg(unix)]
    fn a_symlinked_source_file_is_reported_rather_than_silently_dropped() {
        let tmp = workspace();
        std::os::unix::fs::symlink(tmp.path().join("src/main.rs"), tmp.path().join("link.rs"))
            .unwrap();

        let result = scan(tmp.path(), &ScanOptions::default()).unwrap();
        assert!(
            !result.files.iter().any(|f| f.path.ends_with("link.rs")),
            "the link itself is not a second copy of the file"
        );
        let (path, reason) = result
            .skipped_symlinks
            .iter()
            .find(|(p, _)| p.ends_with("link.rs"))
            .expect("the skipped link is listed");
        assert!(path.ends_with("link.rs"));
        assert!(
            reason.contains("symlink to") && reason.contains("main.rs"),
            "the reason names the target: {reason}"
        );
    }

    #[test]
    fn results_are_deterministically_ordered() {
        let tmp = workspace();
        let a = scan(tmp.path(), &ScanOptions::default()).unwrap();
        let b = scan(tmp.path(), &ScanOptions::default()).unwrap();
        assert_eq!(a.files, b.files);
        assert!(a.files.windows(2).all(|w| w[0].path <= w[1].path));
    }
}
