//! Copy-paste detection: the same code, written twice.
//!
//! The comparison is structural and not textual: a subtree's hash comes from the node kinds it
//! contains. So a copy whose variables were renamed still matches the original, and a textual
//! search would not find it. [`Options::exact`] narrows to copies that also agree on every
//! identifier and literal.
//!
//! Two rules bound the output. A clone must be at least [`Options::min_tokens`] tokens, since
//! every language has small shapes that repeat everywhere. And only maximal clones are
//! reported: a duplicated function also duplicates its body, its statements and its
//! expressions, and listing all of them buries the finding.

use crate::index::Index;
use crate::lang::Language;
use crate::model::FactGap;
use crate::parse::Parsers;
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// One occurrence of duplicated code.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Clone {
    pub file: PathBuf,
    pub span: Span,
    /// First and last line it covers, 1-based and inclusive.
    pub start_line: usize,
    pub end_line: usize,
    /// The columns to go with them, so this is a range `fr extract` accepts. Without
    /// them a caller had to open the file and measure the line itself, which is the one
    /// thing it came here to avoid.
    pub start_col: usize,
    pub end_col: usize,
}

/// A set of places that all say the same thing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CloneClass {
    pub language: Language,
    /// Tokens in each instance, the size of the duplication.
    pub tokens: usize,
    /// Every occurrence, in scan order. Always two or more.
    pub instances: Vec<Clone>,
}

impl CloneClass {
    /// Tokens that would stop being written twice if this were factored out.
    ///
    /// One copy has to stay, so the saving is what the others cost.
    pub fn redundant_tokens(&self) -> usize {
        self.tokens * (self.instances.len() - 1)
    }
}

/// What counts as a duplicate.
#[derive(Debug, Clone)]
pub struct Options {
    /// Smallest clone to report, in tokens.
    pub min_tokens: usize,
    /// Require identifiers and literals to match, not only the structure.
    pub exact: bool,
    /// Restrict the report to these languages. Empty means all of them.
    pub languages: Vec<Language>,
    /// Restrict the report to these path prefixes. Empty means the whole workspace.
    pub paths: Vec<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            // Around a dozen lines of ordinary code. Below this the matches are
            // language boilerplate, an import block, a struct literal, a `for` over
            // a slice, which repeat by nature and are not duplication anyone can act
            // on.
            min_tokens: 60,
            exact: false,
            languages: Vec::new(),
            paths: Vec::new(),
        }
    }
}

/// A subtree, hashed and located.
struct Candidate {
    hash: u64,
    file_index: usize,
    span: Span,
    tokens: usize,
    /// Depth from the file root, so parents are considered before their children.
    depth: usize,
}

/// Find duplicated code across the workspace.
pub fn find(index: &Index, options: &Options) -> Result<Vec<CloneClass>> {
    let parsers = Parsers::new();
    let mut files: Vec<(PathBuf, Language, String)> = Vec::new();

    for (path, info) in index.files() {
        if !options.languages.is_empty() && !options.languages.contains(&info.language) {
            continue;
        }
        if !options.paths.is_empty() && !options.paths.iter().any(|p| path.starts_with(p)) {
            continue;
        }
        let Ok(source) = crate::vfs::read_to_string(path) else {
            continue;
        };
        files.push((path.to_path_buf(), info.language, source));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut candidates: Vec<Candidate> = Vec::new();
    for (i, (_, language, source)) in files.iter().enumerate() {
        let Ok(parsed) = parsers.parse(*language, source) else {
            // A file that does not parse has no reliable structure to compare. It is
            // already reported by `fr parse`, so it is skipped and not guessed at.
            continue;
        };
        collect(parsed.root(), source, i, options, &mut candidates);
    }

    // Group by hash. The language is carried per file, so two languages can only
    // share a class if their grammars agree on every node kind, which they do not.
    let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, candidate) in candidates.iter().enumerate() {
        by_hash.entry(candidate.hash).or_default().push(i);
    }

    let mut groups: Vec<Vec<usize>> = by_hash
        .into_values()
        .filter(|members| members.len() > 1)
        .collect();
    // Biggest first, and by position within a size so the output is stable.
    groups.sort_by_key(|members| {
        let first = &candidates[members[0]];
        (
            std::cmp::Reverse(first.tokens),
            first.depth,
            first.file_index,
            first.span.start,
        )
    });

    // Only maximal clones. A duplicated function duplicates every statement inside
    // it; once the function is reported, its parts are the same finding said again.
    let mut covered: Vec<Vec<Span>> = vec![Vec::new(); files.len()];
    let mut classes = Vec::new();

    for members in groups {
        let mut instances = Vec::new();
        let mut claimed: Vec<(usize, Span)> = Vec::new();
        for &m in &members {
            let candidate = &candidates[m];
            let inside_a_reported_clone = covered[candidate.file_index]
                .iter()
                .any(|s| s.start <= candidate.span.start && candidate.span.end <= s.end);
            if inside_a_reported_clone {
                continue;
            }
            // Two instances of one class must not be the same bytes twice.
            if claimed
                .iter()
                .any(|(f, s)| *f == candidate.file_index && overlaps(*s, candidate.span))
            {
                continue;
            }
            claimed.push((candidate.file_index, candidate.span));
            instances.push(m);
        }
        if instances.len() < 2 {
            continue;
        }

        let first = &candidates[instances[0]];
        let mut class = CloneClass {
            language: files[first.file_index].1,
            tokens: first.tokens,
            instances: Vec::new(),
        };
        for m in instances {
            let candidate = &candidates[m];
            let (path, _, source) = &files[candidate.file_index];
            covered[candidate.file_index].push(candidate.span);
            let lines = LineIndex::new(source);
            let from = lines.line_col(candidate.span.start, source);
            // The end is exclusive, so the last covered byte is one before it. The column
            // reported is one past that, which a range wants.
            let to = lines.line_col(candidate.span.end.saturating_sub(1), source);
            class.instances.push(Clone {
                file: path.clone(),
                span: candidate.span,
                start_line: from.line,
                end_line: to.line,
                start_col: from.col,
                end_col: to.col + 1,
            });
        }
        class.instances.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then_with(|| a.span.start.cmp(&b.span.start))
        });
        classes.push(class);
    }

    classes.sort_by_key(|c| {
        (
            std::cmp::Reverse(c.redundant_tokens()),
            c.instances[0].file.clone(),
            c.instances[0].span.start,
        )
    });
    Ok(classes)
}

fn overlaps(a: Span, b: Span) -> bool {
    a.start < b.end && b.start < a.end
}

/// Hash every subtree large enough to be worth comparing.
///
/// Done bottom-up in one pass: a node's hash is built from its kind and the hashes of its
/// children. So the whole file costs one traversal instead of one per subtree.
fn collect(
    root: Node<'_>,
    source: &str,
    file_index: usize,
    options: &Options,
    out: &mut Vec<Candidate>,
) {
    // (node, depth, whether its children have been processed)
    let mut stack: Vec<(Node<'_>, usize, bool)> = vec![(root, 0, false)];
    let mut hashes: HashMap<usize, (u64, usize)> = HashMap::new();

    while let Some((node, depth, expanded)) = stack.pop() {
        if !expanded {
            stack.push((node, depth, true));
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                stack.push((child, depth + 1, false));
            }
            continue;
        }

        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();

        let (hash, tokens) = if children.is_empty() {
            // A leaf is one token. In structural mode its kind is all that matters,
            // so a renamed copy still matches; in exact mode the text matters too.
            let mut h = fnv(node.kind().as_bytes());
            if options.exact {
                h = mix(h, fnv(Span::from(node).text(source).as_bytes()));
            }
            (h, 1)
        } else {
            let mut h = fnv(node.kind().as_bytes());
            let mut tokens = 0usize;
            for child in &children {
                let (child_hash, child_tokens) = hashes
                    .get(&child.id())
                    .copied()
                    .unwrap_or_else(|| (fnv(child.kind().as_bytes()), 1));
                h = mix(h, child_hash);
                tokens += child_tokens;
            }
            (h, tokens)
        };
        hashes.insert(node.id(), (hash, tokens));

        if tokens >= options.min_tokens && node.is_named() {
            out.push(Candidate {
                hash,
                file_index,
                span: Span::from(node),
                tokens,
                depth,
            });
        }
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

/// Order-dependent combination, so `f(a, b)` and `f(b, a)` differ.
fn mix(accumulated: u64, next: u64) -> u64 {
    let mut h = accumulated ^ next.wrapping_add(0x9e37_79b9_7f4a_7c15);
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h
}

/// Is duplicate detection meaningful for this language?
///
/// Files the report skipped because they do not parse.
pub fn unparsed(index: &Index, options: &Options) -> Vec<PathBuf> {
    index
        .files()
        .filter(|(path, info)| {
            info.gaps.contains(&FactGap::SyntaxErrors)
                && (options.languages.is_empty() || options.languages.contains(&info.language))
                && (options.paths.is_empty() || options.paths.iter().any(|p| path.starts_with(p)))
        })
        .map(|(path, _)| path.to_path_buf())
        .collect()
}

/// Convenience for callers that only have a path prefix and defaults.
pub fn find_in(index: &Index, root: &Path) -> Result<Vec<CloneClass>> {
    crate::capabilities::record_workspace(crate::capabilities::Capability::Duplicates, index);
    find(
        index,
        &Options {
            paths: vec![root.to_path_buf()],
            ..Options::default()
        },
    )
}
