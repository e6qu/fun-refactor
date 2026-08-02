//! Rename a symbol and every reference that provably points at it.
//!
//! What gets rewritten is decided by resolution confidence, never by name matching:
//! `exact` and `import-qualified` references are edited, anything weaker is reported
//! as a warning for a human to check. The same is true of textual occurrences in
//! strings and comments, which no amount of syntax analysis can resolve.

use super::{Refusal, Warning, WarningKind};
use crate::edit::{Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::{Symbol, SymbolId};
use crate::parse::Parsers;
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

/// A rename that has been worked out but not applied.
#[derive(Debug)]
pub struct RenamePlan {
    pub old_name: String,
    pub new_name: String,
    pub edits: EditSet,
    pub warnings: Vec<Warning>,
    /// Number of reference sites rewritten, excluding the definition itself.
    pub reference_edits: usize,
}

/// Work out how to rename `symbol` to `new_name`.
///
/// Returns a [`Refusal`] rather than a partial rename when the change would collide
/// with an existing name or the new name is not valid for the language.
pub fn plan(index: &Index, symbol_id: SymbolId, new_name: &str) -> Result<RenamePlan> {
    let symbol = index
        .symbol(symbol_id)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    validate_name(new_name, symbol.language)?;

    if new_name == symbol.name {
        anyhow::bail!("'{new_name}' is already the name of this symbol");
    }

    check_collision(index, symbol, new_name)?;

    let mut edits = EditSet::new();
    let mut warnings = Vec::new();

    // The definition itself.
    edits.add(
        symbol.file.clone(),
        Edit::new(
            symbol.name_span,
            new_name,
            format!("rename definition of {}", symbol.name),
        ),
    );

    // References that resolved strongly enough to rewrite.
    let mut reference_edits = 0;
    for reference in index.references_to(symbol_id) {
        if reference.confidence.is_safe_to_rewrite() {
            edits.add(
                reference.file.clone(),
                Edit::new(
                    reference.span,
                    new_name,
                    format!("rename reference to {}", symbol.name),
                ),
            );
            reference_edits += 1;
        } else {
            warnings.push(locate_warning(
                WarningKind::WeaklyResolved,
                &reference.file,
                reference.span.start,
                format!(
                    "reference resolved only as '{}'; left unchanged",
                    reference.confidence.as_str()
                ),
            ));
        }
    }

    // Same-named references that resolved somewhere else entirely. These are not
    // ours to touch, but a human should confirm the resolution was right.
    for reference in index.unresolved_matching(symbol_id) {
        if reference.target.is_none() {
            warnings.push(locate_warning(
                WarningKind::WeaklyResolved,
                &reference.file,
                reference.span.start,
                format!("unresolved occurrence of '{}'; left unchanged", symbol.name),
            ));
        }
    }

    // Strings and comments: invisible to any analysis, so report every hit.
    warnings.extend(textual_sweep(index, &symbol.name)?);

    // Files that did not parse cleanly may be hiding references.
    for (path, info) in index.files() {
        if info.had_parse_errors {
            warnings.push(Warning {
                kind: WarningKind::ParseErrors,
                file: path.clone(),
                line: 1,
                col: 1,
                detail: "file has syntax errors; references in it may be missing".into(),
            });
        }
    }

    warnings.sort_by(|a, b| {
        (a.kind.as_str(), &a.file, a.line, a.col).cmp(&(b.kind.as_str(), &b.file, b.line, b.col))
    });
    warnings.dedup();

    Ok(RenamePlan {
        old_name: symbol.name.clone(),
        new_name: new_name.to_string(),
        edits,
        warnings,
        reference_edits,
    })
}

/// Reject names that are not valid identifiers for the language.
fn validate_name(name: &str, language: Language) -> Result<(), Refusal> {
    if name.is_empty() {
        return Err(Refusal::InvalidName {
            name: name.into(),
            reason: "a name cannot be empty".into(),
        });
    }

    // Config and markup languages allow far more in a name (CSS classes may contain
    // dashes, YAML keys may contain almost anything), so only the imperative
    // languages get identifier rules.
    let strict = matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Zig
            | Language::TypeScript
            | Language::Tsx
            | Language::Python
            | Language::Bash
    );

    if strict {
        let mut chars = name.chars();
        let first = chars.next().expect("checked non-empty above");
        if !(first.is_alphabetic() || first == '_') {
            return Err(Refusal::InvalidName {
                name: name.into(),
                reason: "must start with a letter or underscore".into(),
            });
        }
        if let Some(bad) = chars.find(|c| !(c.is_alphanumeric() || *c == '_')) {
            return Err(Refusal::InvalidName {
                name: name.into(),
                reason: format!("contains '{bad}', which is not allowed in an identifier"),
            });
        }
        if is_keyword(name, language) {
            return Err(Refusal::InvalidName {
                name: name.into(),
                reason: format!("'{name}' is a {language} keyword"),
            });
        }
    } else if name.chars().any(|c| c.is_whitespace()) {
        return Err(Refusal::InvalidName {
            name: name.into(),
            reason: "must not contain whitespace".into(),
        });
    }

    Ok(())
}

/// Reserved words that cannot be used as identifiers.
fn is_keyword(name: &str, language: Language) -> bool {
    const SHARED: &[&str] = &["if", "else", "for", "while", "return", "break", "continue"];
    let specific: &[&str] = match language {
        Language::Rust => &[
            "fn", "let", "mut", "const", "struct", "enum", "impl", "trait", "match", "use", "pub",
            "mod", "self", "Self", "super", "crate", "move", "ref", "static", "type", "where",
            "unsafe", "async", "await", "dyn", "loop", "in", "as",
        ],
        Language::Go => &[
            "func", "var", "const", "type", "struct", "interface", "map", "chan", "go", "defer",
            "select", "switch", "case", "default", "package", "import", "range", "fallthrough",
        ],
        Language::Python => &[
            "def", "class", "lambda", "import", "from", "global", "nonlocal", "pass", "raise",
            "try", "except", "finally", "with", "yield", "assert", "del", "None", "True", "False",
            "and", "or", "not", "is", "in", "as", "async", "await", "elif",
        ],
        Language::TypeScript | Language::Tsx => &[
            "function", "var", "let", "const", "class", "interface", "type", "enum", "import",
            "export", "default", "extends", "implements", "new", "this", "super", "typeof",
            "instanceof", "void", "null", "undefined", "async", "await", "yield", "switch",
            "case", "try", "catch", "finally", "throw", "delete", "in", "of",
        ],
        Language::Zig => &[
            "fn", "var", "const", "pub", "struct", "enum", "union", "error", "try", "catch",
            "defer", "comptime", "inline", "test", "switch", "orelse", "unreachable", "and", "or",
        ],
        Language::Bash => &["function", "then", "fi", "do", "done", "case", "esac", "in"],
        _ => &[],
    };
    SHARED.contains(&name) || specific.contains(&name)
}

/// Refuse when the new name is already defined where the renamed symbol is visible.
fn check_collision(index: &Index, symbol: &Symbol, new_name: &str) -> Result<(), Refusal> {
    let existing = index.find_symbols(new_name, Some(&symbol.file));
    for other in existing {
        // A collision matters when the two could be visible at the same point: the
        // same scope, or either one at file level.
        let same_scope = other.scope == symbol.scope;
        let either_top_level = other.container.is_none() || symbol.container.is_none();
        if same_scope || (either_top_level && other.kind == symbol.kind) {
            return Err(Refusal::NameCollision {
                existing: new_name.to_string(),
                file: other.file.clone(),
            });
        }
    }
    Ok(())
}

/// Find the old name inside string literals and comments across the workspace.
///
/// These are exactly the references that defeat both syntax analysis and language
/// servers, so they are surfaced for review and never edited automatically.
fn textual_sweep(index: &Index, name: &str) -> Result<Vec<Warning>> {
    let parsers = Parsers::new();
    let mut warnings = Vec::new();

    for (path, info) in index.files() {
        let Ok(source) = std::fs::read_to_string(path) else {
            continue;
        };
        if !source.contains(name) {
            continue;
        }
        let parsed = parsers.parse(info.language, &source)?;
        let line_index = LineIndex::new(&source);

        for span in string_and_comment_spans(&parsed) {
            let text = span.text(&source);
            for (offset, _) in text.match_indices(name) {
                if !is_word_boundary(text, offset, name.len()) {
                    continue;
                }
                let pos = line_index.line_col(span.start + offset, &source);
                warnings.push(Warning {
                    kind: WarningKind::TextualOccurrence,
                    file: path.clone(),
                    line: pos.line,
                    col: pos.col,
                    detail: format!("'{name}' appears in a string or comment; left unchanged"),
                });
            }
        }
    }
    Ok(warnings)
}

/// Spans of string literals, comments and Helm template actions.
fn string_and_comment_spans(parsed: &crate::parse::Parsed) -> Vec<Span> {
    let mut spans: Vec<Span> = parsed.template_actions.clone();
    let mut cursor = parsed.root().walk();
    let mut recurse = true;

    loop {
        let node = cursor.node();
        let kind = node.kind();
        // Grammars name these differently: string_literal, raw_string_literal,
        // interpreted_string_literal, line_comment, block_comment, comment…
        if kind.contains("string") || kind.contains("comment") || kind.contains("char_literal") {
            spans.push(Span::from(node));
            recurse = false;
        }
        if recurse && cursor.goto_first_child() {
            continue;
        }
        recurse = true;
        if cursor.goto_next_sibling() {
            continue;
        }
        loop {
            if !cursor.goto_parent() {
                spans.sort();
                spans.dedup();
                return spans;
            }
            if cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Is the match at `offset` a whole word rather than part of a longer one?
fn is_word_boundary(haystack: &str, offset: usize, len: usize) -> bool {
    let before_ok = haystack[..offset]
        .chars()
        .next_back()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    let after_ok = haystack[offset + len..]
        .chars()
        .next()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    before_ok && after_ok
}

fn locate_warning(kind: WarningKind, file: &PathBuf, offset: usize, detail: String) -> Warning {
    let (line, col) = match std::fs::read_to_string(file) {
        Ok(source) => {
            let pos = LineIndex::new(&source).line_col(offset, &source);
            (pos.line, pos.col)
        }
        Err(_) => (0, 0),
    };
    Warning {
        kind,
        file: file.clone(),
        line,
        col,
        detail,
    }
}

/// Group warnings by kind for reporting.
pub fn group_warnings(warnings: &[Warning]) -> HashMap<&'static str, Vec<&Warning>> {
    let mut grouped: HashMap<&'static str, Vec<&Warning>> = HashMap::new();
    for w in warnings {
        grouped.entry(w.kind.as_str()).or_default().push(w);
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use crate::scan::{ScanResult, SourceFile};

    fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
        let tmp = tempfile::tempdir().unwrap();
        let mut scanned = ScanResult::default();
        for (name, content) in files {
            let path = tmp.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, content).unwrap();
            scanned.files.push(SourceFile {
                language: crate::lang::detect(&path).unwrap(),
                path,
            });
        }
        let index = Index::build_from_scan(&scanned).unwrap();
        (tmp, index)
    }

    fn only_symbol(index: &Index, name: &str) -> SymbolId {
        let found = index.find_symbols(name, None);
        assert_eq!(found.len(), 1, "expected one '{name}', got {found:?}");
        found[0].id
    }

    #[test]
    fn renames_definition_and_all_exact_references() {
        let src = "fn helper() {}\nfn main() { helper(); helper(); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = plan(&index, only_symbol(&index, "helper"), "assist").unwrap();

        assert_eq!(plan.reference_edits, 2);
        let path = tmp.path().join("a.rs");
        let edits = plan.edits.edits_for(&path).unwrap();
        let out = apply_to_string(src, edits).unwrap();
        assert_eq!(out, "fn assist() {}\nfn main() { assist(); assist(); }\n");
    }

    #[test]
    fn rename_only_touches_the_identifier_not_the_line() {
        // Everything around the identifier must be preserved byte-for-byte.
        let src = "fn  helper ( ) { } // helper stays in this comment\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = plan(&index, only_symbol(&index, "helper"), "x").unwrap();
        let path = tmp.path().join("a.rs");
        let out = apply_to_string(src, plan.edits.edits_for(&path).unwrap()).unwrap();
        assert_eq!(out, "fn  x ( ) { } // helper stays in this comment\n");
    }

    #[test]
    fn does_not_rename_same_name_in_another_file() {
        // Two independent `parse` functions: renaming one must not touch the other.
        let (tmp, index) = workspace(&[
            ("a.rs", "fn parse() {}\nfn a() { parse(); }\n"),
            ("b.rs", "fn parse() {}\nfn b() { parse(); }\n"),
        ]);
        let a_path = tmp.path().join("a.rs");
        let target = index
            .find_symbols("parse", Some(&a_path))
            .first()
            .unwrap()
            .id;

        let plan = plan(&index, target, "decode").unwrap();
        assert!(plan.edits.edits_for(&a_path).is_some());
        assert!(
            plan.edits.edits_for(&tmp.path().join("b.rs")).is_none(),
            "must not edit the other file's identically-named function"
        );
    }

    #[test]
    fn refuses_to_collide_with_an_existing_name() {
        let (_tmp, index) = workspace(&[("a.rs", "fn alpha() {}\nfn beta() {}\n")]);
        let err = plan(&index, only_symbol(&index, "alpha"), "beta").unwrap_err();
        let refusal = err.downcast_ref::<Refusal>().expect("a refusal");
        assert!(matches!(refusal, Refusal::NameCollision { .. }), "{refusal}");
    }

    #[test]
    fn refuses_invalid_identifiers() {
        let (_tmp, index) = workspace(&[("a.rs", "fn alpha() {}\n")]);
        let id = only_symbol(&index, "alpha");

        for bad in ["", "2fast", "has space", "has-dash"] {
            let err = plan(&index, id, bad).unwrap_err();
            assert!(
                err.downcast_ref::<Refusal>()
                    .is_some_and(|r| matches!(r, Refusal::InvalidName { .. })),
                "expected InvalidName for {bad:?}, got {err}"
            );
        }
    }

    #[test]
    fn refuses_language_keywords() {
        let (_tmp, index) = workspace(&[("a.rs", "fn alpha() {}\n")]);
        let err = plan(&index, only_symbol(&index, "alpha"), "impl").unwrap_err();
        assert!(err.to_string().contains("keyword"), "got: {err}");
    }

    #[test]
    fn refuses_a_no_op_rename() {
        let (_tmp, index) = workspace(&[("a.rs", "fn alpha() {}\n")]);
        let err = plan(&index, only_symbol(&index, "alpha"), "alpha").unwrap_err();
        assert!(err.to_string().contains("already the name"), "got: {err}");
    }

    #[test]
    fn reports_occurrences_in_strings_and_comments_without_editing_them() {
        let src = "// helper does things\nfn helper() {}\nfn main() { let s = \"call helper now\"; }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let plan = plan(&index, only_symbol(&index, "helper"), "assist").unwrap();

        let textual: Vec<_> = plan
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::TextualOccurrence)
            .collect();
        assert_eq!(textual.len(), 2, "got {textual:?}");

        // The strings and comments must survive untouched.
        let path = tmp.path().join("a.rs");
        let out = apply_to_string(src, plan.edits.edits_for(&path).unwrap()).unwrap();
        assert!(out.contains("// helper does things"));
        assert!(out.contains("\"call helper now\""));
        assert!(out.contains("fn assist()"));
    }

    #[test]
    fn textual_sweep_respects_word_boundaries() {
        // `helperful` contains `helper` but is a different word.
        let src = "fn helper() {}\n// helperful and helper_x are not matches, helper is\n";
        let (_tmp, index) = workspace(&[("a.rs", src)]);
        let plan = plan(&index, only_symbol(&index, "helper"), "assist").unwrap();
        let textual: Vec<_> = plan
            .warnings
            .iter()
            .filter(|w| w.kind == WarningKind::TextualOccurrence)
            .collect();
        assert_eq!(textual.len(), 1, "got {textual:?}");
    }

    #[test]
    fn shadowed_variable_rename_stays_in_its_scope() {
        let src = "fn f() {\n    let x = 1;\n    {\n        let x = 2;\n        g(x);\n    }\n    h(x);\n}\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        // Target the INNER x by position.
        let inner_offset = src.rfind("let x").unwrap() + 4;
        let inner = index.definition_at(&path, inner_offset).unwrap();
        let plan = plan(&index, inner.id, "y").unwrap();
        let out = apply_to_string(src, plan.edits.edits_for(&path).unwrap()).unwrap();

        // g(x) used the inner binding and must change; h(x) used the outer one.
        assert!(out.contains("let y = 2;"), "got:\n{out}");
        assert!(out.contains("g(y)"), "inner use should be renamed:\n{out}");
        assert!(out.contains("h(x)"), "outer use must be untouched:\n{out}");
        assert!(out.contains("let x = 1;"), "outer binding untouched:\n{out}");
    }

    #[test]
    fn rename_is_reversible() {
        // A→B→A must return the original bytes exactly.
        let src = "fn alpha() {}\nfn main() { alpha(); }\n";
        let (tmp, index) = workspace(&[("a.rs", src)]);
        let path = tmp.path().join("a.rs");

        let forward = plan(&index, only_symbol(&index, "alpha"), "renamed").unwrap();
        let once = apply_to_string(src, forward.edits.edits_for(&path).unwrap()).unwrap();
        std::fs::write(&path, &once).unwrap();

        let mut scanned = ScanResult::default();
        scanned.files.push(SourceFile {
            path: path.clone(),
            language: Language::Rust,
        });
        let index2 = Index::build_from_scan(&scanned).unwrap();
        let back = plan(&index2, only_symbol(&index2, "renamed"), "alpha").unwrap();
        let twice = apply_to_string(&once, back.edits.edits_for(&path).unwrap()).unwrap();

        assert_eq!(twice, src);
    }

    #[test]
    fn warns_about_files_that_failed_to_parse() {
        let (_tmp, index) = workspace(&[
            ("a.rs", "fn alpha() {}\n"),
            ("broken.rs", "fn oops( {\n"),
        ]);
        let plan = plan(&index, only_symbol(&index, "alpha"), "beta").unwrap();
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.kind == WarningKind::ParseErrors),
            "a file with syntax errors may hide references and must be reported"
        );
    }

    #[test]
    fn config_languages_allow_names_identifiers_would_reject() {
        // A CSS class may contain dashes, which is not a valid Rust identifier.
        assert!(validate_name("my-class", Language::Css).is_ok());
        assert!(validate_name("my-class", Language::Rust).is_err());
        // Whitespace is never acceptable.
        assert!(validate_name("two words", Language::Css).is_err());
    }
}
