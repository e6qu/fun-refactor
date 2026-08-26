//! A string that names a file, and the file it names.
//!
//! A CI step runs `./scripts/deploy.sh`. A Terraform resource renders
//! `templatefile("${path.module}/init.sh", …)`. A compose service builds from
//! `./docker/Dockerfile`. Each is a path written as a string in one language
//! naming a file in another, and none of them resolved. The string reached
//! nothing, and the script it named looked unused.
//!
//! The question a path answers is small and exact. The file either exists in
//! the workspace or it does not, so there is no guessing to do and nothing to
//! report as a maybe. What it answers is "what runs this?", which is the
//! question asked before deleting a script and before moving one.
//!
//! What is *not* claimed: the flags after the path. `--namespace signals` names
//! an option the script declares, and matching one to the other needs the
//! script's own argument parsing read. That is a separate edge and it is not
//! this one.

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
    ///
    /// The one failure a path edge can report, and the reason to report at
    /// all. A CI step running a script nobody kept is a build that breaks on
    /// the next push and not before.
    pub fn is_dangling(&self) -> bool {
        self.names.is_none()
    }
}

/// Every path-valued string in the workspace, and the file each names.
///
/// The root is passed rather than taken from the index. A path written against
/// the workspace and a path written beside the file are both ordinary, and only
/// the caller knows where the workspace begins.
pub fn links(index: &Index, root: &Path) -> Result<Vec<PathLink>> {
    let mut out = Vec::new();
    for (file, _) in index.files() {
        let Some(language) = crate::lang::detect(file) else {
            continue;
        };
        // Only the languages that name a file by writing its path. A Rust
        // string holding a path is a runtime value, and treating one as an
        // edge would report every log message that mentions a directory.
        if !matches!(
            language,
            Language::Yaml
                | Language::Helm
                | Language::Hcl
                | Language::Json
                | Language::Markdown
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
///
/// Read from the text and not from a query. The shapes are per *framework* and
/// not per language. A `run:` step and a `templatefile(…)` call have nothing in
/// common but the string in the middle.
fn written_paths(source: &str, language: Language) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (at, line) in source.lines().enumerate() {
        let number = at + 1;
        match language {
            // `- run: ./scripts/deploy.sh --namespace signals`, and the
            // `script:` GitLab spells it with. The path is the first word.
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
            // `[the ingest module](src/ingest.rs)`. Documentation drifts from
            // code more reliably than anything else in a repository. A link to
            // a file somebody moved is where the drift shows first.
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
///
/// The fragment comes off. `guide.md#intro` names `guide.md`, and the heading
/// inside it is a separate edge the anchor resolution already follows.
fn markdown_destinations(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < line.len() {
        // A destination follows `](`, the one shape both an inline link and an
        // inline image write.
        let Some(found) = line[at..].find("](") else { break };
        let start = at + found + 2;
        let Some(end) = line[start..].find(')') else { break };
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
///
/// A path has a separator or an extension, and is not a URL and not a shell
/// word. `make` is a command, `./build.sh` is a file, and `npm run build` names
/// neither. Asking about the shape and not about whether the file happens to
/// exist is what lets a dangling path be reported rather than dropped.
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
///
/// A relative path is relative to the file, then to the workspace root. Both
/// are how the tools that read these files resolve one. Trying the file's own
/// directory first lets `./init.sh` beside a `.tf` find its neighbour.
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
