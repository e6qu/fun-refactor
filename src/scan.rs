//! Workspace discovery: find the files we can act on.

use crate::lang::Language;
use anyhow::Result;
use ignore::WalkBuilder;
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
            // almost always vendored or generated. Skipping them is reported, not silent.
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
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let Some(language) = crate::lang::detect(path) else {
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
    fn results_are_deterministically_ordered() {
        let tmp = workspace();
        let a = scan(tmp.path(), &ScanOptions::default()).unwrap();
        let b = scan(tmp.path(), &ScanOptions::default()).unwrap();
        assert_eq!(a.files, b.files);
        assert!(a.files.windows(2).all(|w| w[0].path <= w[1].path));
    }
}
