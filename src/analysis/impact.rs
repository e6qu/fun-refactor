//! Blast radius: everything a change to one symbol could touch.
//!
//! This is the query that motivates a multi-language tool. It composes every edge
//! layer at once — resolved references, call edges, string-keyed cross-language
//! references and textual occurrences — and groups the result by how certain each
//! finding is, so "what breaks if I change this" has an answer that spans the
//! code/config boundary no single-language tool can see across.

use crate::analysis::call_graph::CallGraph;
use crate::index::Index;
use crate::lang::Language;
use crate::model::{Confidence, SymbolId};
use crate::span::{LineCol, LineIndex, Span};
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// One thing that would be affected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Impacted {
    pub file: PathBuf,
    pub language: Language,
    pub line: usize,
    pub col: usize,
    pub kind: ImpactKind,
    pub confidence: Confidence,
    pub detail: String,
}

/// How something is affected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactKind {
    /// A resolved reference to the symbol.
    Reference,
    /// A function that calls it, directly or transitively.
    Caller,
    /// A reference from another language (CSS class in HTML, id in a label).
    CrossLanguage,
    /// The name appears in a string, comment or template.
    Textual,
}

impl ImpactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ImpactKind::Reference => "reference",
            ImpactKind::Caller => "caller",
            ImpactKind::CrossLanguage => "cross-language",
            ImpactKind::Textual => "textual",
        }
    }
}

/// The full blast radius of a symbol.
#[derive(Debug, Clone)]
pub struct Impact {
    pub symbol: SymbolId,
    pub items: Vec<Impacted>,
    /// How many callers the depth limit stopped short of.
    ///
    /// A bound nobody is told about reads as a complete answer, and this is the command
    /// a person uses to decide whether a change is safe. A five-deep call chain traced
    /// three levels reported "affects 4 site(s)" and said nothing about the two it had
    /// not looked at.
    pub callers_beyond_the_depth_limit: usize,
}

impl Impact {
    /// Files touched, in order.
    pub fn files(&self) -> Vec<&PathBuf> {
        let mut files: Vec<&PathBuf> = self.items.iter().map(|i| &i.file).collect();
        files.sort();
        files.dedup();
        files
    }

    /// Languages touched — more than one means no language server could answer this.
    pub fn languages(&self) -> Vec<Language> {
        let mut langs: Vec<Language> = self.items.iter().map(|i| i.language).collect();
        langs.sort();
        langs.dedup();
        langs
    }

    pub fn by_kind(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for item in &self.items {
            *counts.entry(item.kind.as_str()).or_default() += 1;
        }
        counts
    }

    pub fn by_confidence(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for item in &self.items {
            *counts.entry(item.confidence.as_str()).or_default() += 1;
        }
        counts
    }

    /// Items that would definitely need to change.
    pub fn certain(&self) -> Vec<&Impacted> {
        self.items
            .iter()
            .filter(|i| i.confidence.is_safe_to_rewrite() && i.kind != ImpactKind::Textual)
            .collect()
    }

    /// Items a human has to judge.
    pub fn needs_review(&self) -> Vec<&Impacted> {
        self.items
            .iter()
            .filter(|i| !i.confidence.is_safe_to_rewrite() || i.kind == ImpactKind::Textual)
            .collect()
    }
}

/// Compute the blast radius of `symbol`.
///
/// `caller_depth` bounds how far call edges are followed; 0 disables the call walk.
pub fn analyse(index: &Index, symbol: SymbolId, caller_depth: usize) -> Result<Impact> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    let mut items = Vec::new();
    let mut sources: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut locate = |file: &PathBuf, offset: usize| -> LineCol {
        let source = sources
            .entry(file.clone())
            .or_insert_with(|| crate::vfs::read_to_string(file).unwrap_or_default());
        LineIndex::new(source).line_col(offset, source)
    };

    // Every definition site of the entity, and every reference to it.
    for id in index.definition_group(symbol) {
        for reference in index.references_to(id) {
            let at = locate(&reference.file, reference.span.start);
            // A reference from another language is the interesting case.
            let kind = if reference.language != sym.language {
                ImpactKind::CrossLanguage
            } else {
                ImpactKind::Reference
            };
            items.push(Impacted {
                file: reference.file.clone(),
                language: reference.language,
                line: at.line,
                col: at.col,
                kind,
                confidence: reference.confidence,
                detail: format!("{} of {}", kind.as_str(), sym.name),
            });
        }
    }

    // Transitive callers, when the symbol is callable.
    let mut callers_beyond_the_depth_limit = 0;
    if caller_depth > 0 && sym.kind.is_callable() {
        let graph = CallGraph::build(index);
        let trace = graph.trace(
            symbol,
            crate::analysis::call_graph::Direction2::Callers,
            caller_depth,
        );
        callers_beyond_the_depth_limit = trace.unexplored.len();
        for node in trace.nodes.iter().filter(|n| n.depth > 0) {
            let Some(caller) = index.symbol(node.symbol) else {
                continue;
            };
            let at = locate(&caller.file, caller.name_span.start);
            items.push(Impacted {
                file: caller.file.clone(),
                language: caller.language,
                line: at.line,
                col: at.col,
                kind: ImpactKind::Caller,
                confidence: node.caller.map(|(_, c)| c).unwrap_or(Confidence::Exact),
                detail: format!(
                    "{} calls it (depth {})",
                    caller.qualified_name(),
                    node.depth
                ),
            });
        }
    }

    // Same-named occurrences that did not resolve here.
    for reference in index.unresolved_matching(symbol) {
        if reference.target.is_none() {
            let at = locate(&reference.file, reference.span.start);
            items.push(Impacted {
                file: reference.file.clone(),
                language: reference.language,
                line: at.line,
                col: at.col,
                kind: ImpactKind::Textual,
                confidence: reference.confidence,
                detail: format!("unresolved occurrence of {}", sym.name),
            });
        }
    }

    items.sort_by(|a, b| (a.kind, &a.file, a.line, a.col).cmp(&(b.kind, &b.file, b.line, b.col)));
    items.dedup();

    Ok(Impact {
        symbol,
        items,
        callers_beyond_the_depth_limit,
    })
}

/// Render a human-readable report.
pub fn format_report(index: &Index, impact: &Impact) -> String {
    let name = index
        .symbol(impact.symbol)
        .map(|s| s.qualified_name())
        .unwrap_or_else(|| "<unknown>".into());

    let mut out = format!(
        "{} affects {} site(s) across {} file(s) and {} language(s)\n",
        name,
        impact.items.len(),
        impact.files().len(),
        impact.languages().len()
    );

    let certain = impact.certain();
    if !certain.is_empty() {
        out.push_str(&format!("\nWould definitely change ({}):\n", certain.len()));
        for item in certain.iter().take(40) {
            out.push_str(&format!(
                "  {:<14} {}:{}:{}\n",
                item.kind.as_str(),
                item.file.display(),
                item.line,
                item.col
            ));
        }
        if certain.len() > 40 {
            out.push_str(&format!("  … and {} more\n", certain.len() - 40));
        }
    }

    if impact.callers_beyond_the_depth_limit > 0 {
        out.push_str(&format!(
            "\nThis is what the search reached, not everything there is: the caller \
             depth stopped it at {} function(s) that are themselves called from \
             elsewhere. Raise --caller-depth to see further.\n",
            impact.callers_beyond_the_depth_limit
        ));
    }

    let review = impact.needs_review();
    if !review.is_empty() {
        out.push_str(&format!("\nNeeds review ({}):\n", review.len()));
        for item in review.iter().take(20) {
            out.push_str(&format!(
                "  {:<14} {}:{}:{}  [{}] {}\n",
                item.kind.as_str(),
                item.file.display(),
                item.line,
                item.col,
                item.confidence.as_str(),
                item.detail
            ));
        }
        if review.len() > 20 {
            out.push_str(&format!("  … and {} more\n", review.len() - 20));
        }
    }
    out
}

/// The span of a symbol's name, for callers that want to highlight it.
pub fn name_span(index: &Index, symbol: SymbolId) -> Option<Span> {
    index.symbol(symbol).map(|s| s.name_span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{scan, ScanOptions};

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            crate::vfs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    #[test]
    fn reports_references_and_callers() {
        let (_tmp, index) = workspace(&[(
            "a.rs",
            "fn leaf() {}\nfn middle() { leaf(); }\nfn top() { middle(); }\n",
        )]);
        let leaf = index.find_symbols("leaf", None)[0].id;
        let impact = analyse(&index, leaf, 5).unwrap();

        assert!(impact.items.iter().any(|i| i.kind == ImpactKind::Reference));
        // `top` calls `middle` calls `leaf`, so both are in the radius.
        let callers: Vec<_> = impact
            .items
            .iter()
            .filter(|i| i.kind == ImpactKind::Caller)
            .collect();
        assert_eq!(callers.len(), 2, "got {callers:?}");
    }

    #[test]
    fn caller_depth_zero_skips_the_call_walk() {
        let (_tmp, index) = workspace(&[("a.rs", "fn leaf() {}\nfn top() { leaf(); }\n")]);
        let leaf = index.find_symbols("leaf", None)[0].id;
        let impact = analyse(&index, leaf, 0).unwrap();
        assert!(!impact.items.iter().any(|i| i.kind == ImpactKind::Caller));
    }

    #[test]
    fn cross_language_references_are_labelled_as_such() {
        // This is the query no language server can answer.
        let (_tmp, index) = workspace(&[
            ("styles.css", ".card { color: red; }\n"),
            ("index.html", "<div class=\"card\"></div>\n"),
            (
                "src/App.tsx",
                "export const A = () => <b className=\"card\" />;\n",
            ),
        ]);
        let card = index.find_symbols("card", None)[0].id;
        let impact = analyse(&index, card, 0).unwrap();

        let cross: Vec<_> = impact
            .items
            .iter()
            .filter(|i| i.kind == ImpactKind::CrossLanguage)
            .collect();
        assert_eq!(cross.len(), 2, "got {cross:?}");

        // And which two. The count alone is satisfied by any pair — including the CSS
        // definition itself mislabelled, with the TSX missed entirely, which is the
        // failure this query exists to make impossible.
        let mut from: Vec<&str> = cross
            .iter()
            .filter_map(|i| i.file.file_name().and_then(|n| n.to_str()))
            .collect();
        from.sort();
        assert_eq!(from, ["App.tsx", "index.html"], "got {cross:?}");

        // Three languages in one answer.
        assert!(
            impact.languages().len() >= 2,
            "expected several languages: {:?}",
            impact.languages()
        );
    }

    #[test]
    fn certain_and_needs_review_are_separated() {
        let (_tmp, index) = workspace(&[(
            "a.rs",
            "fn target() {}\nfn caller() { target(); }\n// target in a comment\n",
        )]);
        let target = index.find_symbols("target", None)[0].id;
        let impact = analyse(&index, target, 1).unwrap();

        assert!(!impact.certain().is_empty());
        // Everything certain is safe to rewrite; everything else is flagged.
        assert!(impact
            .certain()
            .iter()
            .all(|i| i.confidence.is_safe_to_rewrite()));
    }

    #[test]
    fn an_unused_symbol_has_an_empty_radius() {
        let (_tmp, index) = workspace(&[("a.rs", "fn orphan() {}\n")]);
        let orphan = index.find_symbols("orphan", None)[0].id;
        let impact = analyse(&index, orphan, 5).unwrap();
        assert!(impact.items.is_empty(), "got {:?}", impact.items);
    }

    #[test]
    fn report_mentions_files_and_languages() {
        let (_tmp, index) = workspace(&[
            ("styles.css", ".card { color: red; }\n"),
            ("index.html", "<div class=\"card\"></div>\n"),
        ]);
        let card = index.find_symbols("card", None)[0].id;
        let impact = analyse(&index, card, 0).unwrap();
        let report = format_report(&index, &impact);
        assert!(report.contains("language(s)"), "got:\n{report}");
        assert!(report.contains("index.html"), "got:\n{report}");
    }
}
