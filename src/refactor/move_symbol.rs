//! Move a top-level symbol to another file, updating imports.
//!
//! Updating the imports *is* the refactoring — moving the text is trivial. That means
//! the operation is only offered where an import statement can actually be computed
//! from two paths: TypeScript and Python relative imports. Elsewhere it refuses,
//! because a move that leaves dangling references is worse than no move at all
//! (PLAN.md D8).

use super::Refusal;
use crate::edit::{full_line_span, Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::{SymbolId, SymbolKind};
use crate::span::Span;
use anyhow::Result;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A move worked out but not applied.
#[derive(Debug)]
pub struct MovePlan {
    pub symbol: String,
    pub from: PathBuf,
    pub to: PathBuf,
    pub edits: EditSet,
    /// Files that gained an import.
    pub imports_added: Vec<PathBuf>,
}

/// Move `symbol` into `destination`.
pub fn to_file(index: &Index, symbol: SymbolId, destination: &Path) -> Result<MovePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    if !supports_move(sym.language) {
        return Err(Refusal::Unsupported {
            operation: "move to file".into(),
            language: format!(
                "{} — an import statement cannot be derived from file paths in this language",
                sym.language
            ),
        }
        .into());
    }

    if sym.container.is_some() {
        anyhow::bail!(
            "'{}' is nested inside another definition; only top-level symbols can be moved",
            sym.name
        );
    }

    if destination == sym.file {
        anyhow::bail!("'{}' is already in {}", sym.name, destination.display());
    }

    let source = std::fs::read_to_string(&sym.file)?;

    // Take the whole line(s) so the moved text carries its own formatting and the
    // hole left behind does not become a blank line.
    let start_line = full_line_span(&source, sym.full_span.start);
    let end_line = full_line_span(&source, sym.full_span.end.saturating_sub(1));
    let removal = Span::new(start_line.start, end_line.end.max(sym.full_span.end));
    let moved_text = removal.text(&source).to_string();

    let mut edits = EditSet::new();
    edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move {} out", sym.name)),
    );

    // Append to the destination, which may not exist yet.
    let existing = std::fs::read_to_string(destination).unwrap_or_default();
    let separator = if existing.is_empty() || existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let insert_at = Span::new(existing.len(), existing.len());
    edits.add(
        destination.to_path_buf(),
        Edit::new(
            insert_at,
            format!("{separator}{moved_text}"),
            format!("move {} in", sym.name),
        ),
    );

    // Every file that referenced it now needs an import — including the file it
    // came from, if references remain there.
    let mut needs_import: BTreeSet<PathBuf> = BTreeSet::new();
    for reference in index.references_to(symbol) {
        if reference.file != *destination {
            needs_import.insert(reference.file.clone());
        }
    }

    let mut imports_added = Vec::new();
    for file in &needs_import {
        let Some(statement) = import_statement(sym.language, file, destination, &sym.name) else {
            continue;
        };
        let target_source = std::fs::read_to_string(file).unwrap_or_default();
        let insert = import_insertion_point(&target_source);
        edits.add(
            file.clone(),
            Edit::new(
                Span::new(insert, insert),
                statement,
                format!("import {} from its new home", sym.name),
            ),
        );
        imports_added.push(file.clone());
    }

    Ok(MovePlan {
        symbol: sym.name.clone(),
        from: sym.file.clone(),
        to: destination.to_path_buf(),
        edits,
        imports_added,
    })
}

/// Languages where an import statement can be computed from two paths.
fn supports_move(language: Language) -> bool {
    matches!(
        language,
        Language::TypeScript | Language::Tsx | Language::Python
    )
}

/// The import statement `from` needs in order to see `name` defined in `to`.
fn import_statement(
    language: Language,
    from: &Path,
    to: &Path,
    name: &str,
) -> Option<String> {
    let module = relative_module(from, to)?;
    Some(match language {
        Language::TypeScript | Language::Tsx => {
            format!("import {{ {name} }} from '{module}';\n")
        }
        Language::Python => format!("from {module} import {name}\n"),
        _ => return None,
    })
}

/// A module path for `to`, expressed relative to `from`.
fn relative_module(from: &Path, to: &Path) -> Option<String> {
    let from_dir = from.parent()?;
    let stem = to.file_stem()?.to_str()?;
    let to_dir = to.parent()?;

    if from_dir == to_dir {
        return Some(format!("./{stem}"));
    }

    // Walk up from `from_dir` until the destination is underneath.
    let mut ups = 0;
    let mut probe = from_dir;
    loop {
        if let Ok(rest) = to_dir.strip_prefix(probe) {
            let mut path = if ups == 0 {
                ".".to_string()
            } else {
                vec![".."; ups].join("/")
            };
            for part in rest.components() {
                path.push('/');
                path.push_str(part.as_os_str().to_str()?);
            }
            path.push('/');
            path.push_str(stem);
            return Some(path);
        }
        probe = probe.parent()?;
        ups += 1;
        if ups > 16 {
            return None;
        }
    }
}

/// Where a new import should go: after any existing leading imports.
fn import_insertion_point(source: &str) -> usize {
    let mut offset = 0;
    let mut last_import_end = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
            last_import_end = offset + line.len();
        } else if !trimmed.is_empty() && last_import_end > 0 {
            break;
        }
        offset += line.len();
    }
    last_import_end
}

/// Symbols eligible to be moved.
pub fn movable(index: &Index, file: &Path) -> Vec<SymbolId> {
    let Some(info) = index.file(file) else {
        return Vec::new();
    };
    info.symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| {
            s.container.is_none()
                && matches!(
                    s.kind,
                    SymbolKind::Function | SymbolKind::Class | SymbolKind::Constant
                )
        })
        .map(|s| s.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::apply_to_string;
    use crate::scan::{scan, ScanOptions};

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

    fn apply(plan: &MovePlan, path: &Path) -> String {
        let original = std::fs::read_to_string(path).unwrap_or_default();
        match plan.edits.edits_for(path) {
            Some(edits) => apply_to_string(&original, edits).unwrap(),
            None => original,
        }
    }

    #[test]
    fn moves_a_function_between_python_modules() {
        let (tmp, index) = workspace(&[
            ("helpers.py", "def keep():\n    pass\n\ndef moved():\n    return 1\n"),
            ("other.py", "def existing():\n    pass\n"),
        ]);
        let id = index.find_symbols("moved", None)[0].id;
        let dest = tmp.path().join("other.py");

        let plan = to_file(&index, id, &dest).unwrap();

        let from = apply(&plan, &tmp.path().join("helpers.py"));
        assert!(!from.contains("def moved"), "should be gone:\n{from}");
        assert!(from.contains("def keep"), "should remain:\n{from}");

        let to = apply(&plan, &dest);
        assert!(to.contains("def moved"), "should arrive:\n{to}");
        assert!(to.contains("def existing"), "should be preserved:\n{to}");
    }

    #[test]
    fn adds_an_import_where_the_symbol_is_still_used() {
        let (tmp, index) = workspace(&[
            ("lib.py", "def shared():\n    return 1\n"),
            ("app.py", "from lib import shared\n\ndef use():\n    return shared()\n"),
            ("dest.py", "x = 1\n"),
        ]);
        let id = index.find_symbols("shared", None)[0].id;
        let dest = tmp.path().join("dest.py");
        let plan = to_file(&index, id, &dest).unwrap();

        assert!(
            plan.imports_added.iter().any(|p| p.ends_with("app.py")),
            "app.py uses it and must gain an import: {:?}",
            plan.imports_added
        );
        let app = apply(&plan, &tmp.path().join("app.py"));
        assert!(app.contains("from ./dest import shared") || app.contains("from .dest import shared") || app.contains("dest import shared"),
            "got:\n{app}");
    }

    #[test]
    fn typescript_gets_a_named_import() {
        let (tmp, index) = workspace(&[
            ("a.ts", "export function moved() { return 1; }\n"),
            ("b.ts", "import { moved } from './a';\nexport const x = moved();\n"),
            ("c.ts", "export const y = 2;\n"),
        ]);
        let id = index.find_symbols("moved", None)[0].id;
        let dest = tmp.path().join("c.ts");
        let plan = to_file(&index, id, &dest).unwrap();

        let b = apply(&plan, &tmp.path().join("b.ts"));
        assert!(b.contains("import { moved } from './c';"), "got:\n{b}");
    }

    #[test]
    fn refuses_languages_without_computable_imports() {
        let (tmp, index) = workspace(&[("a.rs", "fn thing() {}\n"), ("b.rs", "\n")]);
        let id = index.find_symbols("thing", None)[0].id;
        let err = to_file(&index, id, &tmp.path().join("b.rs")).unwrap_err();
        assert!(
            err.downcast_ref::<Refusal>()
                .is_some_and(|r| matches!(r, Refusal::Unsupported { .. })),
            "got: {err}"
        );
    }

    #[test]
    fn refuses_to_move_a_nested_symbol() {
        let (tmp, index) = workspace(&[
            ("a.py", "class C:\n    def method(self):\n        pass\n"),
            ("b.py", "x = 1\n"),
        ]);
        let id = index
            .find_symbols("method", None)
            .first()
            .expect("method extracted")
            .id;
        let err = to_file(&index, id, &tmp.path().join("b.py"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("top-level"), "got: {err}");
    }

    #[test]
    fn refuses_a_move_to_the_same_file() {
        let (tmp, index) = workspace(&[("a.py", "def f():\n    pass\n")]);
        let id = index.find_symbols("f", None)[0].id;
        let err = to_file(&index, id, &tmp.path().join("a.py"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("already in"), "got: {err}");
    }

    #[test]
    fn relative_module_paths_are_computed_correctly() {
        assert_eq!(
            relative_module(Path::new("/w/src/a.ts"), Path::new("/w/src/b.ts")).as_deref(),
            Some("./b")
        );
        let nested =
            relative_module(Path::new("/w/src/deep/a.ts"), Path::new("/w/src/b.ts"));
        assert!(
            nested.as_deref().is_some_and(|m| m.contains("b")),
            "got {nested:?}"
        );
    }

    #[test]
    fn import_insertion_lands_after_existing_imports() {
        let source = "import a from 'a';\nimport b from 'b';\n\nconst x = 1;\n";
        let at = import_insertion_point(source);
        assert_eq!(&source[..at], "import a from 'a';\nimport b from 'b';\n");
    }

    #[test]
    fn a_file_with_no_imports_gets_one_at_the_top() {
        assert_eq!(import_insertion_point("const x = 1;\n"), 0);
    }
}
