//! Value flow: where does this value come from, and where does it go.
//!
//! # What tier this is
//!
//! This is syntactic def-use analysis over the resolved index, not a full dataflow
//! engine. It follows assignments and initialisers through resolved references, which
//! answers most "where did this come from" questions in practice, and it stops
//! **loudly** at every boundary it cannot cross — unresolved names, calls whose target
//! is not proven, and dynamic dispatch (PLAN.md D5). It never over-approximates by
//! assuming a value flows through an unknown call.
//!
//! Each step records the confidence of the resolution that produced it, so a chain
//! containing a weak link is visibly weak rather than silently wrong.

use crate::index::Index;
use crate::lang::LanguageClass;
use crate::model::{Confidence, ReferenceKind, SymbolId};
use crate::parse::Parsers;
use crate::span::Span;
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// Which way to follow the flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowDirection {
    /// Where did this value come from?
    Backward,
    /// Where does this value end up?
    Forward,
}

/// Why a flow chain stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// Reached a literal or a parameter — the origin.
    Origin(String),
    /// The value passes through a call whose target did not resolve.
    UnresolvedCall(String),
    /// The name did not resolve to any definition.
    Unresolved(String),
    /// Resolution was too weak to follow honestly.
    TooWeak(Confidence),
    /// The depth limit was reached; more may lie beyond.
    DepthLimit,
    /// The value crosses a function boundary, which this tier does not follow.
    CrossesFunctionBoundary(String),
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StopReason::Origin(what) => write!(f, "origin: {what}"),
            StopReason::UnresolvedCall(name) => {
                write!(
                    f,
                    "value comes from call to '{name}', which did not resolve"
                )
            }
            StopReason::Unresolved(name) => write!(f, "'{name}' did not resolve to a definition"),
            StopReason::TooWeak(c) => {
                write!(f, "resolution only '{}'; not followed", c.as_str())
            }
            StopReason::DepthLimit => write!(f, "depth limit reached; more may lie beyond"),
            StopReason::CrossesFunctionBoundary(name) => write!(
                f,
                "'{name}' crosses a function boundary; inter-procedural flow is not followed here"
            ),
        }
    }
}

/// One hop in a flow chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowStep {
    pub symbol: Option<SymbolId>,
    /// What the source says at this point.
    pub text: String,
    pub file: PathBuf,
    pub span: Span,
    pub depth: usize,
    /// How well the reference that led here resolved.
    pub confidence: Confidence,
}

/// The result of a flow query.
#[derive(Debug, Clone)]
pub struct FlowResult {
    pub direction: FlowDirection,
    pub steps: Vec<FlowStep>,
    /// Every boundary the walk refused to cross, so gaps are visible.
    pub stops: Vec<(usize, StopReason)>,
}

impl FlowResult {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The weakest link in the chain — the honest confidence of the whole answer.
    pub fn weakest_confidence(&self) -> Option<Confidence> {
        self.steps.iter().map(|s| s.confidence).max()
    }

    pub fn format_tree(&self) -> String {
        let mut out = String::new();
        for step in &self.steps {
            let marker = if step.confidence.is_safe_to_rewrite() {
                String::new()
            } else {
                format!(" [{}]", step.confidence.as_str())
            };
            out.push_str(&format!(
                "{}{}  ({}){}\n",
                "  ".repeat(step.depth),
                step.text.trim(),
                step.file.display(),
                marker
            ));
        }
        if !self.stops.is_empty() {
            out.push_str("\nStopped at:\n");
            for (depth, reason) in &self.stops {
                out.push_str(&format!("{}- {reason}\n", "  ".repeat(*depth)));
            }
        }
        out
    }
}

/// Trace where the value at `offset` comes from.
pub fn backward(index: &Index, file: &Path, offset: usize, max_depth: usize) -> Result<FlowResult> {
    let mut result = FlowResult {
        direction: FlowDirection::Backward,
        steps: Vec::new(),
        stops: Vec::new(),
    };
    let mut seen = HashSet::new();
    walk_backward(index, file, offset, 0, max_depth, &mut result, &mut seen)?;
    Ok(result)
}

fn walk_backward(
    index: &Index,
    file: &Path,
    offset: usize,
    depth: usize,
    max_depth: usize,
    result: &mut FlowResult,
    seen: &mut HashSet<(PathBuf, usize)>,
) -> Result<()> {
    if depth > max_depth {
        result.stops.push((depth, StopReason::DepthLimit));
        return Ok(());
    }
    if !seen.insert((file.to_path_buf(), offset)) {
        return Ok(());
    }

    let Some(info) = index.file(file) else {
        return Ok(());
    };
    let source = std::fs::read_to_string(file)?;
    let parsed = Parsers::new().parse(info.language, &source)?;

    // Find the definition this position refers to.
    let Some(symbol) = index.definition_at(file, offset) else {
        let name = index
            .reference_at(file, offset)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| "<unknown>".into());
        result.stops.push((depth, StopReason::Unresolved(name)));
        return Ok(());
    };

    result.steps.push(FlowStep {
        symbol: Some(symbol.id),
        text: line_text(&source, symbol.name_span.start),
        file: symbol.file.clone(),
        span: symbol.name_span,
        depth,
        confidence: Confidence::Exact,
    });

    // A parameter's value comes from the caller — an inter-procedural step this
    // tier deliberately does not take.
    if symbol.kind == crate::model::SymbolKind::Parameter {
        result.stops.push((
            depth,
            StopReason::CrossesFunctionBoundary(symbol.name.clone()),
        ));
        return Ok(());
    }

    // The value assigned to this definition.
    let Some(value_node) = value_of_definition(&parsed, symbol.full_span, symbol.name_span) else {
        result.stops.push((
            depth,
            StopReason::Origin(format!("{} has no initialiser", symbol.name)),
        ));
        return Ok(());
    };
    let value_span = Span::from(value_node);

    // Identifiers inside that value are the sources to follow next.
    let contributors: Vec<_> = info
        .references
        .iter()
        .map(|i| &index.references[*i])
        .filter(|r| value_span.contains(r.span))
        .collect();

    if contributors.is_empty() {
        result.stops.push((
            depth,
            StopReason::Origin(format!("literal value {}", value_span.text(&source).trim())),
        ));
        return Ok(());
    }

    for reference in contributors {
        // A call in the initialiser: the value comes from the callee's return.
        if reference.kind == ReferenceKind::Call {
            match reference.target {
                Some(target) if reference.confidence.is_safe_to_rewrite() => {
                    if let Some(callee) = index.symbol(target) {
                        result.steps.push(FlowStep {
                            symbol: Some(target),
                            text: format!("returned by {}", callee.qualified_name()),
                            file: callee.file.clone(),
                            span: callee.name_span,
                            depth: depth + 1,
                            confidence: reference.confidence,
                        });
                        result.stops.push((
                            depth + 1,
                            StopReason::CrossesFunctionBoundary(callee.name.clone()),
                        ));
                    }
                }
                _ => result.stops.push((
                    depth + 1,
                    StopReason::UnresolvedCall(reference.name.clone()),
                )),
            }
            continue;
        }

        if !reference.confidence.is_safe_to_rewrite() {
            result
                .stops
                .push((depth + 1, StopReason::TooWeak(reference.confidence)));
            continue;
        }

        match reference.target.and_then(|t| index.symbol(t)) {
            Some(source_symbol) => {
                let next_file = source_symbol.file.clone();
                let next_offset = source_symbol.name_span.start;
                walk_backward(
                    index,
                    &next_file,
                    next_offset,
                    depth + 1,
                    max_depth,
                    result,
                    seen,
                )?;
            }
            None => result
                .stops
                .push((depth + 1, StopReason::Unresolved(reference.name.clone()))),
        }
    }
    Ok(())
}

/// Trace where a symbol's value is used.
pub fn forward(index: &Index, symbol_id: SymbolId, max_depth: usize) -> Result<FlowResult> {
    let mut result = FlowResult {
        direction: FlowDirection::Forward,
        steps: Vec::new(),
        stops: Vec::new(),
    };
    let mut seen = HashSet::new();
    walk_forward(index, symbol_id, 0, max_depth, &mut result, &mut seen)?;
    Ok(result)
}

fn walk_forward(
    index: &Index,
    symbol_id: SymbolId,
    depth: usize,
    max_depth: usize,
    result: &mut FlowResult,
    seen: &mut HashSet<SymbolId>,
) -> Result<()> {
    if depth > max_depth {
        result.stops.push((depth, StopReason::DepthLimit));
        return Ok(());
    }
    if !seen.insert(symbol_id) {
        return Ok(());
    }
    let Some(symbol) = index.symbol(symbol_id) else {
        return Ok(());
    };

    let source = std::fs::read_to_string(&symbol.file)?;
    result.steps.push(FlowStep {
        symbol: Some(symbol_id),
        text: line_text(&source, symbol.name_span.start),
        file: symbol.file.clone(),
        span: symbol.name_span,
        depth,
        confidence: Confidence::Exact,
    });

    for reference in index.references_to(symbol_id) {
        let Ok(ref_source) = std::fs::read_to_string(&reference.file) else {
            continue;
        };
        result.steps.push(FlowStep {
            symbol: None,
            text: line_text(&ref_source, reference.span.start),
            file: reference.file.clone(),
            span: reference.span,
            depth: depth + 1,
            confidence: reference.confidence,
        });

        // If this use initialises another definition, the value flows onward.
        let Some(info) = index.file(&reference.file) else {
            continue;
        };
        let parsed = Parsers::new().parse(info.language, &ref_source)?;
        if let Some(target) =
            enclosing_assignment_target(index, &parsed, &reference.file, reference.span)
        {
            walk_forward(index, target, depth + 2, max_depth, result, seen)?;
        }
    }

    // Weakly-resolved uses of the same name are reported, never assumed.
    for weak in index.unresolved_matching(symbol_id) {
        if weak.target.is_none() {
            result
                .stops
                .push((depth + 1, StopReason::Unresolved(weak.name.clone())));
        }
    }
    Ok(())
}

/// The value/initialiser subtree of a definition.
///
/// Grammars differ, but the node carrying an assigned value is reliably reachable
/// through a field named `value` or `right`, which covers `let x = …`, `x = …`,
/// `const x = …`, `x: y` and HCL/YAML attribute forms.
fn value_of_definition<'a>(
    parsed: &'a crate::parse::Parsed,
    full_span: Span,
    name_span: Span,
) -> Option<Node<'a>> {
    let node = parsed
        .root()
        .descendant_for_byte_range(full_span.start, full_span.end)?;

    for field in ["value", "right", "default_value"] {
        if let Some(value) = node.child_by_field_name(field) {
            // Guard against a "value" that is actually the name itself.
            if Span::from(value) != name_span {
                return Some(value);
            }
        }
    }

    // Some grammars nest the declarator one level down.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        for field in ["value", "right", "default_value"] {
            if let Some(value) = child.child_by_field_name(field) {
                if Span::from(value) != name_span {
                    return Some(value);
                }
            }
        }
    }
    None
}

/// If `span` sits inside the value of an assignment, the symbol being assigned.
fn enclosing_assignment_target(
    index: &Index,
    parsed: &crate::parse::Parsed,
    file: &Path,
    span: Span,
) -> Option<SymbolId> {
    let mut node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;

    // Walk outwards looking for a node whose `value`/`right` contains our span.
    for _ in 0..16 {
        for field in ["value", "right"] {
            if let Some(value) = node.child_by_field_name(field) {
                if Span::from(value).contains(span) {
                    let target_span = node
                        .child_by_field_name("name")
                        .or_else(|| node.child_by_field_name("left"))
                        .or_else(|| node.child_by_field_name("pattern"))
                        .map(Span::from)?;
                    let info = index.file(file)?;
                    return info
                        .symbols
                        .iter()
                        .filter_map(|id| index.symbol(*id))
                        .find(|s| s.name_span == target_span || s.full_span.contains(target_span))
                        .map(|s| s.id);
                }
            }
        }
        node = node.parent()?;
    }
    None
}

/// The trimmed source line containing `offset`, for display.
fn line_text(source: &str, offset: usize) -> String {
    let index = crate::span::LineIndex::new(source);
    let pos = index.line_col(offset, source);
    index
        .line_span(pos.line)
        .map(|s| s.text(source).trim().to_string())
        .unwrap_or_default()
}

/// Does flow analysis apply to this file's language?
///
/// Config and markup languages get provenance analysis instead of dataflow: their
/// evaluation model is substitution and override, not execution.
pub fn applies_to(index: &Index, file: &Path) -> bool {
    index
        .file(file)
        .is_some_and(|info| info.language.class() == LanguageClass::Imperative)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{ScanResult, SourceFile};

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        let mut scanned = ScanResult::default();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::write(&path, content).unwrap();
            scanned.files.push(SourceFile {
                language: crate::lang::detect(&path).unwrap(),
                path,
            });
        }
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    #[test]
    fn backward_follows_an_assignment_chain() {
        let src = "fn f() {\n    let a = 1;\n    let b = a;\n    let c = b;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let c_offset = src.find("let c").unwrap() + 4;
        let flow = backward(&index, &path, c_offset, 10).unwrap();

        let texts: Vec<&str> = flow.steps.iter().map(|s| s.text.as_str()).collect();
        assert!(texts.iter().any(|t| t.contains("let c = b")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("let b = a")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("let a = 1")), "{texts:?}");
    }

    #[test]
    fn backward_reports_a_literal_origin() {
        let src = "fn f() {\n    let a = 42;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let flow = backward(&index, &path, src.find("let a").unwrap() + 4, 5).unwrap();
        assert!(
            flow.stops
                .iter()
                .any(|(_, r)| matches!(r, StopReason::Origin(_))),
            "reaching a literal is an origin: {:?}",
            flow.stops
        );
    }

    #[test]
    fn backward_stops_loudly_at_a_function_boundary() {
        // A parameter's value comes from callers; this tier must say so, not guess.
        let src = "fn f(input: i32) {\n    let a = input;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let flow = backward(&index, &path, src.find("let a").unwrap() + 4, 5).unwrap();
        assert!(
            flow.stops
                .iter()
                .any(|(_, r)| matches!(r, StopReason::CrossesFunctionBoundary(_))),
            "expected a boundary stop, got {:?}",
            flow.stops
        );
    }

    #[test]
    fn backward_reports_an_unresolved_call_rather_than_following_it() {
        let src = "fn f() {\n    let a = mystery_call();\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let flow = backward(&index, &path, src.find("let a").unwrap() + 4, 5).unwrap();
        assert!(
            flow.stops
                .iter()
                .any(|(_, r)| matches!(r, StopReason::UnresolvedCall(_))),
            "an unresolvable call must be reported, not skipped: {:?}",
            flow.stops
        );
    }

    #[test]
    fn backward_records_a_resolved_call_as_its_source() {
        let src = "fn make() -> i32 { 1 }\nfn f() {\n    let a = make();\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let flow = backward(&index, &path, src.find("let a").unwrap() + 4, 5).unwrap();
        assert!(
            flow.steps
                .iter()
                .any(|s| s.text.contains("returned by make")),
            "got {:?}",
            flow.steps.iter().map(|s| &s.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn forward_finds_every_use() {
        let src = "fn f() {\n    let a = 1;\n    g(a);\n    h(a);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let a = index
            .definition_at(&path, src.find("let a").unwrap() + 4)
            .unwrap();

        let flow = forward(&index, a.id, 5).unwrap();
        let uses = flow.steps.iter().filter(|s| s.text.contains("(a)")).count();
        assert_eq!(uses, 2, "both uses should appear: {:?}", flow.steps);
    }

    #[test]
    fn cycles_terminate() {
        // Mutually referential assignments must not hang the walk.
        let src = "fn f() {\n    let a = b;\n    let b = a;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let flow = backward(&index, &path, src.find("let a").unwrap() + 4, 50).unwrap();
        assert!(flow.steps.len() < 20, "walk should converge");
    }

    #[test]
    fn depth_limit_is_reported() {
        let src = "fn f() {\n    let a = 1;\n    let b = a;\n    let c = b;\n    let d = c;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let flow = backward(&index, &path, src.find("let d").unwrap() + 4, 1).unwrap();
        assert!(
            flow.stops.iter().any(|(_, r)| *r == StopReason::DepthLimit),
            "a truncated walk must say so: {:?}",
            flow.stops
        );
    }

    #[test]
    fn weakest_confidence_summarises_the_chain() {
        let src = "fn f() {\n    let a = 1;\n    let b = a;\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let flow = backward(&index, &path, src.find("let b").unwrap() + 4, 5).unwrap();
        assert_eq!(flow.weakest_confidence(), Some(Confidence::Exact));
    }

    #[test]
    fn flow_does_not_apply_to_config_languages() {
        let (tmp, index) = workspace(&[("main.tf", "variable \"x\" {\n  default = 1\n}\n")]);
        let path = tmp.path().join("main.tf");
        assert!(
            !applies_to(&index, &path),
            "config languages get provenance, not dataflow"
        );
    }
}
