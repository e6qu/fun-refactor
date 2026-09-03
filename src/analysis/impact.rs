//! Blast radius: everything a change to one symbol could touch.

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

    /// Languages touched, more than one means no language server could answer this.
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
pub fn analyse(index: &Index, symbol: SymbolId, caller_depth: usize) -> Result<Impact> {
    let graph = CallGraph::built(index);
    analyse_with_graph(index, symbol, caller_depth, &graph)
}

/// [`analyse`], reusing a call graph the caller already built.
pub fn analyse_with_graph(
    index: &Index,
    symbol: SymbolId,
    caller_depth: usize,
    graph: &CallGraph,
) -> Result<Impact> {
    if let Some(language) = index.symbol(symbol).map(|s| s.language) {
        crate::capabilities::record(crate::capabilities::Capability::Impact, language);
    }
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
        let walk = tainted_callers(graph, symbol, caller_depth);
        callers_beyond_the_depth_limit = walk.stopped_short;
        for (id, confidence, depth) in walk.callers {
            let Some(caller) = index.symbol(id) else {
                continue;
            };
            let at = locate(&caller.file, caller.name_span.start);
            items.push(Impacted {
                file: caller.file.clone(),
                language: caller.language,
                line: at.line,
                col: at.col,
                kind: ImpactKind::Caller,
                confidence,
                detail: format!("{} calls it (depth {})", caller.qualified_name(), depth),
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

    // The name written as text: an `__all__` entry, a docstring, a CI script.
    let accounted: std::collections::HashSet<(PathBuf, usize, usize)> = items
        .iter()
        .map(|item| (item.file.clone(), item.line, item.col))
        .collect();
    for mention in crate::mentions::of(index, &sym.name)? {
        if accounted.contains(&(mention.file.clone(), mention.line, mention.col)) {
            continue;
        }
        let Some(language) = index.file(&mention.file).map(|info| info.language) else {
            continue;
        };
        items.push(Impacted {
            file: mention.file,
            language,
            line: mention.line,
            col: mention.col,
            kind: ImpactKind::Textual,
            confidence: Confidence::NameOnly,
            detail: format!("'{}' is written here as text", sym.name),
        });
    }

    items.sort_by(|a, b| (a.kind, &a.file, a.line, a.col).cmp(&(b.kind, &b.file, b.line, b.col)));
    items.dedup();

    Ok(Impact {
        symbol,
        items,
        callers_beyond_the_depth_limit,
    })
}

/// The callers reached from `start`, each with the confidence of its whole route.
struct TaintedCallers {
    /// `(caller, confidence, depth)`, outward from the symbol.
    callers: Vec<(SymbolId, Confidence, usize)>,
    /// Nodes the depth limit stopped at that still had callers beyond them.
    stopped_short: usize,
}

/// Walk the caller graph carrying a per-route confidence.
fn tainted_callers(graph: &CallGraph, start: SymbolId, max_depth: usize) -> TaintedCallers {
    use std::collections::{HashMap, HashSet, VecDeque};

    let mut best: HashMap<SymbolId, (Confidence, usize)> = HashMap::new();
    let mut stopped: HashSet<SymbolId> = HashSet::new();
    let mut queue: VecDeque<(SymbolId, Confidence, usize)> = VecDeque::new();
    best.insert(start, (Confidence::Exact, 0));
    queue.push_back((start, Confidence::Exact, 0));

    while let Some((id, confidence, depth)) = queue.pop_front() {
        // A node improved after this entry was queued has already re-queued itself.
        if best.get(&id) != Some(&(confidence, depth)) {
            continue;
        }
        let callers = graph.callers(id);
        if depth >= max_depth {
            if !callers.is_empty() {
                stopped.insert(id);
            }
            continue;
        }
        for (caller, edge) in callers {
            // Weakest edge on the route: the tiers order strongest first.
            let route = confidence.max(edge.confidence);
            let stronger = match best.get(&caller) {
                None => true,
                Some((existing, _)) => route < *existing,
            };
            if stronger {
                best.insert(caller, (route, depth + 1));
                queue.push_back((caller, route, depth + 1));
            }
        }
    }

    let mut callers: Vec<(SymbolId, Confidence, usize)> = best
        .into_iter()
        .filter(|(id, _)| *id != start)
        .map(|(id, (confidence, depth))| (id, confidence, depth))
        .collect();
    callers.sort_by_key(|(id, _, depth)| (*depth, *id));
    TaintedCallers {
        callers,
        stopped_short: stopped.len(),
    }
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
            "\nThe search reached this far, which is not everything there is: the caller \
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

    use crate::testing::workspace;

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

        // And which two.
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

    const DISPATCH_CHAIN: &str = "trait Speaker {\n    fn speak(&self) -> String;\n}\n\nstruct Dog;\n\nimpl Speaker for Dog {\n    fn speak(&self) -> String {\n        noise()\n    }\n}\n\nfn noise() -> String {\n    String::from(\"woof\")\n}\n\nfn announce(s: &dyn Speaker) -> String {\n    s.speak()\n}\n\nfn page() -> String {\n    announce(&Dog)\n}\n\nfn render() -> String {\n    page()\n}\n\nfn main() {\n    println!(\"{}\", render());\n}\n";

    fn caller_confidence(impact: &Impact, name: &str) -> Confidence {
        impact
            .items
            .iter()
            .find(|i| i.kind == ImpactKind::Caller && i.detail.starts_with(&format!("{name} ")))
            .unwrap_or_else(|| panic!("no caller {name}: {:?}", impact.items))
            .confidence
    }

    #[test]
    fn an_unproven_edge_taints_everything_reached_only_through_it() {
        // `announce` reaches `speak` through dynamic dispatch, so that edge is a candidate.
        let (_tmp, index) = workspace(&[("chain.rs", DISPATCH_CHAIN)]);
        let noise = index.find_symbols("noise", None)[0].id;
        let impact = analyse(&index, noise, 10).unwrap();

        assert!(
            caller_confidence(&impact, "Dog::speak").is_safe_to_rewrite(),
            "the direct caller is proven"
        );
        for name in ["announce", "page", "render", "main"] {
            assert!(
                !caller_confidence(&impact, name).is_safe_to_rewrite(),
                "{name} sits beyond the dispatch edge and needs review: {:?}",
                impact.items
            );
        }
        let review_names: Vec<&str> = impact
            .needs_review()
            .into_iter()
            .map(|i| i.detail.as_str())
            .collect();
        assert!(
            review_names.iter().any(|d| d.starts_with("render ")),
            "the review list holds the tainted caller: {review_names:?}"
        );
    }

    #[test]
    fn a_fully_resolved_second_route_restores_certainty() {
        // `main` also calls `noise` directly, so the weak dispatch route is not the only way to
        // reach it.
        let source = DISPATCH_CHAIN.replace(
            "fn main() {\n    println!(\"{}\", render());\n}\n",
            "fn main() {\n    noise();\n    println!(\"{}\", render());\n}\n",
        );
        let (_tmp, index) = workspace(&[("chain.rs", &source)]);
        let noise = index.find_symbols("noise", None)[0].id;
        let impact = analyse(&index, noise, 10).unwrap();

        assert!(
            caller_confidence(&impact, "main").is_safe_to_rewrite(),
            "a direct call outweighs the dispatch route: {:?}",
            impact.items
        );
        assert!(
            !caller_confidence(&impact, "render").is_safe_to_rewrite(),
            "render is still only reached across dispatch"
        );
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
