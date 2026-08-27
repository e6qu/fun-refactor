//! A string that names a file, and the file it names.

use crate::index::Index;
use crate::lang::Language;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// One string that names a file, and the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathLink {
    /// The file the string is written in.
    pub from: PathBuf,
    pub line: usize,
    pub language: Language,
    /// The path as it is written.
    pub written: String,
    /// The file it names, where the workspace holds one.
    pub names: Option<PathBuf>,
}

impl PathLink {
    /// Does this name a file the workspace does not hold?
    pub fn is_dangling(&self) -> bool {
        self.names.is_none()
    }
}

/// Every path-valued string in the workspace, and the file each names.
pub fn links(index: &Index, root: &Path) -> Result<Vec<PathLink>> {
    let mut out = Vec::new();
    for (file, _) in index.files() {
        let Some(language) = crate::lang::detect(file) else {
            continue;
        };
        // Only the languages that name a file by writing its path.
        if !matches!(
            language,
            Language::Yaml | Language::Helm | Language::Hcl | Language::Json | Language::Markdown
        ) {
            continue;
        }
        let Ok(source) = crate::vfs::read_to_string(file) else {
            continue;
        };
        for (line, written) in written_paths(&source, language) {
            let names = resolve(root, file, &written);
            out.push(PathLink {
                from: file.to_path_buf(),
                line,
                language,
                written,
                names,
            });
        }
    }
    out.sort_by(|a, b| (&a.from, a.line).cmp(&(&b.from, b.line)));
    Ok(out)
}

/// The paths one file writes, with the line each sits on.
fn written_paths(source: &str, language: Language) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (at, line) in source.lines().enumerate() {
        let number = at + 1;
        match language {
            // `- run: ./scripts/deploy.sh --namespace signals`, and the `script:` GitLab spells
            // it with.
            Language::Yaml | Language::Helm | Language::Json => {
                for word in ["run:", "script:", "entrypoint:", "dockerfile:"] {
                    let Some(at) = line.find(word) else { continue };
                    let rest = line[at + word.len()..].trim();
                    let rest = rest.trim_start_matches(['-', '"', '\'']).trim();
                    let Some(first) = rest.split_whitespace().next() else {
                        continue;
                    };
                    if let Some(path) = as_a_path(first) {
                        out.push((number, path));
                    }
                }
            }
            // `[the ingest module](src/ingest.rs)`.
            Language::Markdown => {
                for written in markdown_destinations(line) {
                    if let Some(path) = as_a_path(&written) {
                        out.push((number, path));
                    }
                }
            }
            // `templatefile("${path.module}/init.sh", …)` and `file(…)`, the
            // two Terraform functions that name a file.
            Language::Hcl => {
                for word in ["templatefile(", "file(", "filebase64("] {
                    let Some(at) = line.find(word) else { continue };
                    let rest = &line[at + word.len()..];
                    let Some(inside) = quoted(rest) else { continue };
                    if let Some(path) = as_a_path(&inside) {
                        out.push((number, path));
                    }
                }
            }
            _ => {}
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Every `(destination)` an inline link on this line names.
fn markdown_destinations(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < line.len() {
        // A destination follows `](`, the one shape both an inline link and an
        // inline image write.
        let Some(found) = line[at..].find("](") else {
            break;
        };
        let start = at + found + 2;
        let Some(end) = line[start..].find(')') else {
            break;
        };
        let inside = &line[start..start + end];
        // A title may follow the destination: `[x](a.md "Title")`.
        let destination = inside.split_whitespace().next().unwrap_or(inside);
        let destination = destination.split('#').next().unwrap_or(destination);
        if !destination.is_empty() {
            out.push(destination.to_string());
        }
        at = start + end + 1;
    }
    out
}

/// The text between the first pair of quotes.
fn quoted(text: &str) -> Option<String> {
    let text = text.trim_start();
    let quote = text.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &text[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

/// Is this word a path a workspace could hold?
fn as_a_path(word: &str) -> Option<String> {
    let word = word.trim().trim_matches(['"', '\'']);
    if word.is_empty() || word.contains("://") {
        return None;
    }
    // A Terraform path is written against `path.module`, which is the directory
    // the file sits in.
    let word = word
        .trim_start_matches("${path.module}")
        .trim_start_matches("${path.root}")
        .trim_start_matches("${path.cwd}");
    let word = word.trim_start_matches('/');
    if word.is_empty() {
        return None;
    }
    let has_separator = word.contains('/');
    let has_extension = Path::new(word)
        .extension()
        .is_some_and(|e| !e.is_empty() && e.len() <= 5);
    match has_separator || has_extension {
        true => Some(word.to_string()),
        false => None,
    }
}

/// The file a written path names, from the file that wrote it.
fn resolve(root: &Path, from: &Path, written: &str) -> Option<PathBuf> {
    let written = written.trim_start_matches("./");
    let beside = from.parent().map(|dir| dir.join(written));
    if let Some(beside) = beside {
        if crate::vfs::exists(&beside) {
            return Some(beside);
        }
    }
    let from_root = root.join(written);
    match crate::vfs::exists(&from_root) {
        true => Some(from_root),
        false => None,
    }
}
