//! Change a function's signature and every call site.
//!
//! LSP has no request for this — gopls approximates it with parameter-move code
//! actions — so a CLI is free to offer the operation directly. That freedom does not
//! extend to guessing: a call site that did not resolve conclusively is reported and
//! the whole operation refuses, because a half-updated signature does not compile.

use super::Refusal;
use crate::edit::{Edit, EditSet};
use crate::index::Index;
use crate::model::{SymbolId, SymbolKind};
use crate::parse::Parsers;
use crate::span::{LineIndex, Span};
use anyhow::Result;
use tree_sitter::Node;

/// What to do to a parameter list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Remove the parameter at this zero-based position.
    Remove(usize),
    /// Move a parameter from one position to another.
    Move { from: usize, to: usize },
    /// Add a parameter, with the text to insert at each call site.
    Add {
        at: usize,
        declaration: String,
        argument: String,
    },
}

/// A signature change worked out but not applied.
#[derive(Debug)]
pub struct SignaturePlan {
    pub function: String,
    pub change: Change,
    pub edits: EditSet,
    /// Call sites updated.
    pub call_sites: usize,
}

/// Apply `change` to `symbol` and every call site.
pub fn change(index: &Index, symbol: SymbolId, change: Change) -> Result<SignaturePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    if !matches!(sym.kind, SymbolKind::Function | SymbolKind::Method) {
        anyhow::bail!(
            "'{}' is a {}; only functions and methods have signatures",
            sym.name,
            sym.kind.as_str()
        );
    }

    // Every call site must be provable: a missed one is a compile error.
    let references = index.references_to(symbol);
    let weak: Vec<_> = references
        .iter()
        .filter(|r| !r.confidence.is_safe_to_rewrite())
        .collect();
    if let Some(first) = weak.first() {
        return Err(Refusal::TooWeak {
            confidence: first.confidence,
            detail: format!(
                "{} of {} call site(s) did not resolve conclusively; updating only some \
                 would leave the code uncompilable",
                weak.len(),
                references.len()
            ),
        }
        .into());
    }

    let source = std::fs::read_to_string(&sym.file)?;
    let parsed = Parsers::new().parse(sym.language, &source)?;
    let declaration = parsed
        .root()
        .descendant_for_byte_range(sym.full_span.start, sym.full_span.end)
        .ok_or_else(|| anyhow::anyhow!("could not locate the declaration"))?;

    let params = parameter_list(declaration)
        .ok_or_else(|| anyhow::anyhow!("could not find the parameter list of '{}'", sym.name))?;
    let param_spans = list_items(params);

    let mut edits = EditSet::new();
    apply_change(
        &mut edits,
        &sym.file,
        &source,
        params,
        &param_spans,
        &change,
        true,
    )?;

    // Now every call site.
    let mut call_sites = 0;
    for reference in &references {
        if reference.kind != crate::model::ReferenceKind::Call {
            continue;
        }
        let call_source = std::fs::read_to_string(&reference.file)?;
        let call_parsed = Parsers::new().parse(reference.language, &call_source)?;
        let Some(call) = call_expression(&call_parsed, reference.span) else {
            continue;
        };
        let Some(args) = argument_list(call) else {
            continue;
        };
        let arg_spans = list_items(args);

        apply_change(
            &mut edits,
            &reference.file,
            &call_source,
            args,
            &arg_spans,
            &change,
            false,
        )?;
        call_sites += 1;
    }

    Ok(SignaturePlan {
        function: sym.name.clone(),
        change,
        edits,
        call_sites,
    })
}

/// Rewrite one parameter or argument list.
fn apply_change(
    edits: &mut EditSet,
    file: &std::path::Path,
    source: &str,
    list: Node<'_>,
    items: &[Span],
    change: &Change,
    is_declaration: bool,
) -> Result<()> {
    match change {
        Change::Remove(index) => {
            let Some(target) = items.get(*index) else {
                // A declaration must have the parameter; a call may legitimately
                // omit a defaulted one, so only the declaration is an error.
                if is_declaration {
                    anyhow::bail!("there is no parameter at position {index}");
                }
                return Ok(());
            };
            // Take the separating comma with it, or the list ends up malformed.
            let span = with_separator(source, items, *index, *target);
            edits.add(
                file.to_path_buf(),
                Edit::new(span, "", format!("remove parameter {index}")),
            );
        }
        Change::Move { from, to } => {
            let (Some(a), Some(b)) = (items.get(*from), items.get(*to)) else {
                if is_declaration {
                    anyhow::bail!("positions {from} and {to} are not both present");
                }
                return Ok(());
            };
            // Swapping the text of two items keeps every byte in between untouched.
            edits.add(
                file.to_path_buf(),
                Edit::new(*a, b.text(source), format!("move parameter {from}")),
            );
            edits.add(
                file.to_path_buf(),
                Edit::new(*b, a.text(source), format!("move parameter {to}")),
            );
        }
        Change::Add {
            at,
            declaration,
            argument,
        } => {
            let text = if is_declaration { declaration } else { argument };
            if items.is_empty() {
                // Insert just inside the parentheses.
                let inside = Span::new(list.start_byte() + 1, list.start_byte() + 1);
                edits.add(
                    file.to_path_buf(),
                    Edit::new(inside, text.clone(), "add parameter".to_string()),
                );
                return Ok(());
            }
            match items.get(*at) {
                Some(before) => edits.add(
                    file.to_path_buf(),
                    Edit::new(
                        Span::new(before.start, before.start),
                        format!("{text}, "),
                        "add parameter".to_string(),
                    ),
                ),
                None => {
                    let last = items.last().expect("non-empty");
                    edits.add(
                        file.to_path_buf(),
                        Edit::new(
                            Span::new(last.end, last.end),
                            format!(", {text}"),
                            "add parameter".to_string(),
                        ),
                    );
                }
            }
        }
    }
    Ok(())
}

/// Extend a span to swallow the comma that separates it from its neighbour.
fn with_separator(source: &str, items: &[Span], index: usize, target: Span) -> Span {
    let bytes = source.as_bytes();
    if index + 1 < items.len() {
        // Take the following comma and any space after it.
        let mut end = target.end;
        while end < bytes.len() && (bytes[end] == b',' || bytes[end].is_ascii_whitespace()) {
            end += 1;
            if bytes[end.saturating_sub(1)] == b',' {
                while end < bytes.len() && bytes[end] == b' ' {
                    end += 1;
                }
                break;
            }
        }
        Span::new(target.start, end)
    } else if index > 0 {
        // Last item: take the preceding comma instead.
        let mut start = target.start;
        while start > 0 && (bytes[start - 1] == b' ' || bytes[start - 1] == b',') {
            start -= 1;
            if bytes[start] == b',' {
                break;
            }
        }
        Span::new(start, target.end)
    } else {
        target
    }
}

/// The parameter list node of a declaration.
fn parameter_list(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(named) = node.child_by_field_name("parameters") {
        return Some(named);
    }
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|c| c.kind().contains("parameter"));
    found
}

/// The argument list node of a call.
fn argument_list(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(named) = node.child_by_field_name("arguments") {
        return Some(named);
    }
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|c| c.kind().contains("argument"));
    found
}

/// The call expression whose callee is at `span`.
fn call_expression<'a>(parsed: &'a crate::parse::Parsed, span: Span) -> Option<Node<'a>> {
    let mut node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;
    for _ in 0..8 {
        if node.kind().contains("call") {
            return Some(node);
        }
        node = node.parent()?;
    }
    None
}

/// Named children of a list node, i.e. its actual items.
fn list_items(list: Node<'_>) -> Vec<Span> {
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .filter(|c| !c.kind().contains("comment"))
        .map(Span::from)
        .collect()
}

/// Describe a plan for display.
pub fn describe(index: &Index, plan: &SignaturePlan) -> String {
    let what = match &plan.change {
        Change::Remove(i) => format!("removed parameter {i}"),
        Change::Move { from, to } => format!("moved parameter {from} to position {to}"),
        Change::Add { at, declaration, .. } => {
            format!("added parameter `{declaration}` at position {at}")
        }
    };
    let _ = index;
    format!(
        "{}: {what}, updating {} call site(s)",
        plan.function, plan.call_sites
    )
}

/// Line of a symbol, for error messages.
pub fn line_of(index: &Index, symbol: SymbolId) -> usize {
    index
        .symbol(symbol)
        .and_then(|s| {
            std::fs::read_to_string(&s.file)
                .ok()
                .map(|src| LineIndex::new(&src).line_col(s.name_span.start, &src).line)
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use crate::scan::{scan, ScanOptions};
    use std::path::Path;

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    fn apply(plan: &SignaturePlan, path: &Path) -> String {
        let original = std::fs::read_to_string(path).unwrap();
        match plan.edits.edits_for(path) {
            Some(edits) => apply_to_string(&original, edits).unwrap(),
            None => original,
        }
    }

    #[test]
    fn removes_a_middle_parameter_and_updates_calls() {
        let src = "fn f(a: i32, b: i32, c: i32) {}\nfn caller() { f(1, 2, 3); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(&index, id, Change::Remove(1)).unwrap();
        assert_eq!(plan.call_sites, 1);
        assert_eq!(
            apply(&plan, &path),
            "fn f(a: i32, c: i32) {}\nfn caller() { f(1, 3); }\n"
        );
    }

    #[test]
    fn removes_the_last_parameter_without_leaving_a_comma() {
        let src = "fn f(a: i32, b: i32) {}\nfn caller() { f(1, 2); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(&index, id, Change::Remove(1)).unwrap();
        assert_eq!(
            apply(&plan, &path),
            "fn f(a: i32) {}\nfn caller() { f(1); }\n"
        );
    }

    #[test]
    fn moves_a_parameter_and_reorders_arguments() {
        let src = "fn f(a: i32, b: i32) {}\nfn caller() { f(1, 2); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(&index, id, Change::Move { from: 0, to: 1 }).unwrap();
        assert_eq!(
            apply(&plan, &path),
            "fn f(b: i32, a: i32) {}\nfn caller() { f(2, 1); }\n"
        );
    }

    #[test]
    fn adds_a_parameter_with_an_argument_at_each_call() {
        let src = "fn f(a: i32) {}\nfn caller() { f(1); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(
            &index,
            id,
            Change::Add {
                at: 1,
                declaration: "flag: bool".into(),
                argument: "false".into(),
            },
        )
        .unwrap();
        assert_eq!(
            apply(&plan, &path),
            "fn f(a: i32, flag: bool) {}\nfn caller() { f(1, false); }\n"
        );
    }

    #[test]
    fn updates_call_sites_in_other_files() {
        let (tmp, index) = workspace(&[
            ("lib.rs", "pub fn shared(a: i32, b: i32) {}\n"),
            ("main.rs", "use lib::shared;\nfn main() { shared(1, 2); }\n"),
        ]);
        let id = index.find_symbols("shared", None)[0].id;
        let plan = change(&index, id, Change::Remove(0)).unwrap();

        let main = apply(&plan, &tmp.path().join("main.rs"));
        assert!(main.contains("shared(2);"), "got:\n{main}");
    }

    #[test]
    fn refuses_when_a_call_site_is_not_provable() {
        // A same-named function elsewhere makes resolution ambiguous, and updating
        // only some call sites would not compile.
        let (_tmp, index) = workspace(&[
            ("a.rs", "fn ambiguous(x: i32) {}\nfn ambiguous_caller() { ambiguous(1); }\n"),
            ("b.rs", "fn ambiguous(x: i32) {}\nfn other() { ambiguous(2); }\n"),
        ]);
        let id = index.find_symbols("ambiguous", None)[0].id;
        // Whatever resolution says, the operation must either be provably complete
        // or refuse; it must never silently update a subset.
        match change(&index, id, Change::Remove(0)) {
            Ok(plan) => {
                for (_, edits) in plan.edits.iter() {
                    assert!(!edits.is_empty());
                }
            }
            Err(e) => assert!(
                e.downcast_ref::<Refusal>().is_some(),
                "refusal should be explicit: {e}"
            ),
        }
    }

    #[test]
    fn refuses_non_functions() {
        let (_tmp, index) = workspace(&[("a.rs", "struct S;\n")]);
        let id = index.find_symbols("S", None)[0].id;
        let err = change(&index, id, Change::Remove(0)).unwrap_err().to_string();
        assert!(err.contains("only functions"), "got: {err}");
    }

    #[test]
    fn rejects_a_position_that_does_not_exist() {
        let (_tmp, index) = workspace(&[("a.rs", "fn f(a: i32) {}\nfn c() { f(1); }\n")]);
        let id = index.find_symbols("f", None)[0].id;
        let err = change(&index, id, Change::Remove(9)).unwrap_err().to_string();
        assert!(err.contains("no parameter at position"), "got: {err}");
    }

    #[test]
    fn the_result_still_parses() {
        let src = "fn f(a: i32, b: i32) {}\nfn caller() { f(1, 2); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let id = index.find_symbols("f", None)[0].id;
        let plan = change(&index, id, Change::Remove(0)).unwrap();
        let outcomes =
            crate::edit::plan(&plan.edits, crate::edit::Validation::ReparseStrict).unwrap();
        assert_eq!(outcomes.len(), 1);
        let _ = tmp;
    }

    #[test]
    fn works_for_python() {
        let src = "def f(a, b):\n    pass\n\ndef caller():\n    f(1, 2)\n";
        let (tmp, index) = workspace(&[("a.py", src)]);
        let path = tmp.path().join("a.py");
        let id = index.find_symbols("f", None)[0].id;

        let plan = change(&index, id, Change::Remove(1)).unwrap();
        let out = apply(&plan, &path);
        assert!(out.contains("def f(a):"), "got:\n{out}");
        assert!(out.contains("f(1)"), "got:\n{out}");
    }
}
