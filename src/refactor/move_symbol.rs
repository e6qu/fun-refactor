//! Move a top-level definition to another file, updating whatever that language's own
//! resolution rules require.
//!
//! What a move must update differs per language, so each language has its own
//! implementation, refusing where it cannot compute the answer (PLAN.md D8):
//!
//! - **TypeScript / Python** — resolution by relative path: the move rewrites every
//!   referencing file with an import derived from the two paths.
//! - **Rust** — resolution is by module path. The destination's module path is derived
//!   from its location under `src/`, checked for reachability through `mod`
//!   declarations, and `use crate::<module>::<name>;` rewritten or inserted. Where the
//!   module structure does not follow, the move refuses and names the ambiguity.
//! - **Go** — a package *is* a directory, so a move inside one needs no updates at all.
//!   Across packages the symbol must be exported and an import path must be derivable
//!   from `go.mod`.
//! - **HCL / Terraform** — a module *is* a directory, so every address survives a move
//!   between `.tf` files in the same directory and nothing else changes. Across
//!   directories the module changes and every address breaks, so that is refused.
//! - **CSS** — a class is named globally, so no reference changes. Reachability can
//!   break: if nothing `@import`s the destination from where the rule was, the styles
//!   stop applying. The move warns rather than refuses.
//! - **Markdown** — a section is a heading and everything under it up to the next
//!   heading of the same or higher level. In-repo links to the anchors that left
//!   repoint at the new document.
//! - **Zig** — a file is a namespace reached through `const other = @import("other.zig")`,
//!   so a moved declaration becomes `other.thing`. The move writes the `@import` where
//!   one is missing and qualifies the bare uses.
//! - **Bash** — no import binds a name, only `source`, which splices a whole script
//!   in. Every surviving caller of a moved function must source its new home.
//! - **YAML / Helm** — a values key's path does not mention its file, so moving a
//!   top-level key between values files changes no reference. It can change whether
//!   the file is loaded, which the move warns about.
//!
//! HTML and XML refuse: no other document imports an element's name, so there is no
//! reference to repoint and no reachability to preserve.

use super::Refusal;
use crate::edit::{full_line_span, line_indent, Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::{anchor_slug as slug, Symbol, SymbolId, SymbolKind};
use crate::span::{LineIndex, Span};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// A move worked out but not applied.
#[derive(Debug)]
pub struct MovePlan {
    pub symbol: String,
    pub from: PathBuf,
    pub to: PathBuf,
    pub edits: EditSet,
    /// Files that gained an import, or — in Markdown, which has none — whose links
    /// were repointed at the new document.
    pub imports_added: Vec<PathBuf>,
    /// Things the move could not fix and a human must check. A warning never blocks
    /// the move; it says what the tool saw and declined to act on.
    pub warnings: Vec<String>,
}

impl MovePlan {
    fn new(sym: &Symbol, destination: &Path) -> Self {
        MovePlan {
            symbol: sym.name.clone(),
            from: sym.file.clone(),
            to: destination.to_path_buf(),
            edits: EditSet::new(),
            imports_added: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

/// Can a moved definition still be reached from its old use sites in this language?
///
/// Updating the references is the refactoring, so a language qualifies only when that
/// is expressible: an import statement derivable from two paths, or a scope where a
/// move changes no name at all.
pub fn supports_move(language: Language) -> bool {
    why_not_move(language).is_none()
}

/// Why a move is not a thing in this language, if it is not.
///
/// The single authority, because the capability table and the operation itself were
/// deciding this separately: the table said Java could be moved and the operation
/// refused it, which is the table lying about the tool in the tool's own words.
pub fn why_not_move(language: Language) -> Option<&'static str> {
    match language {
        // A document does not import another's elements, so a moved element has no
        // reference anywhere to update.
        Language::Html | Language::Xml => Some(
            "an element has no name that another document imports, so moving one \
             between files changes what each document *is* rather than where a \
             definition lives",
        ),
        // Java ties a file's name to the public type inside it and imports by
        // fully-qualified name rather than by path, so moving a type is a rename of the
        // file *and* of its package, and moving a method is a change of receiver.
        // Neither is the operation this performs, and doing half of it would leave a
        // tree that does not compile.
        Language::Java => Some(
            "a public type must live in a file named after it and imports name packages \
             rather than paths, so moving one is a rename of the file and its package, \
             not a move of a definition",
        ),
        _ => None,
    }
}

/// Move `symbol` into `destination`.
pub fn to_file(index: &Index, symbol: SymbolId, destination: &Path) -> Result<MovePlan> {
    let sym = index
        .symbol(symbol)
        .ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;

    if destination == sym.file {
        bail!("'{}' is already in {}", sym.name, destination.display());
    }

    let Some(dest_language) = crate::lang::detect(destination) else {
        bail!(
            "the destination {} has no recognised language, so '{}' cannot be moved into it",
            destination.display(),
            sym.name
        );
    };
    if !interchangeable(sym.language, dest_language) {
        bail!(
            "'{}' is {}, but the destination {} is {}; a move cannot change language",
            sym.name,
            sym.language,
            destination.display(),
            dest_language
        );
    }

    if let Some(why) = why_not_move(sym.language) {
        return Err(Refusal::Unsupported {
            operation: "move to file".into(),
            language: sym.language,
            because: why.to_string(),
        }
        .into());
    }

    match sym.language {
        Language::TypeScript | Language::Tsx | Language::Python => {
            move_by_relative_import(index, sym, destination)
        }
        Language::Rust => move_rust(index, sym, destination),
        Language::Go => move_go(index, sym, destination),
        Language::Hcl => move_hcl(index, sym, destination),
        Language::Css | Language::Scss => move_css(index, sym, destination),
        Language::Markdown => move_markdown(index, sym, destination),
        Language::Zig => move_zig(index, sym, destination),
        Language::Bash => move_bash(index, sym, destination),
        Language::Yaml | Language::Helm => move_values_key(index, sym, destination),
        // Java ties a file's name to the public type inside it and imports by
        // fully-qualified name rather than by path, so moving a type is a rename of
        // the file *and* of its package, and moving a method is a change of receiver.
        // Neither is the operation this performs, and doing half of it would leave a
        // tree that does not compile.
        Language::Java => Err(Refusal::Unsupported {
            operation: "move to file".into(),
            language: Language::Java,
            because: "a public type must live in a file named after it and imports name \
                      packages rather than paths, so moving one is a rename of the file \
                      and its package, not a move of a definition"
                .into(),
        }
        .into()),
        // An element is addressed by its position in one document, or by an id that
        // every other document reaches through a URL rather than an import. There is
        // no reference a move could repoint and no reachability it could preserve.
        other @ (Language::Html | Language::Xml) => Err(Refusal::Unsupported {
            operation: "move to file".into(),
            language: other,
            because: "an element has no name that another document imports, so moving \
                      one between files changes what each document *is* rather than \
                      where a definition lives"
                .into(),
        }
        .into()),
    }
}

/// May a symbol written in `from` be moved into a file of language `to`?
fn interchangeable(from: Language, to: Language) -> bool {
    from == to
        || matches!(
            (from, to),
            (Language::TypeScript, Language::Tsx)
                | (Language::Tsx, Language::TypeScript)
                | (Language::Css, Language::Scss)
                | (Language::Scss, Language::Css)
        )
}

// ---------------------------------------------------------------------------
// TypeScript and Python: resolution follows relative paths.
// ---------------------------------------------------------------------------

fn move_by_relative_import(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    if sym.container.is_some() {
        bail!(
            "'{}' is nested inside another definition; only top-level symbols can be moved",
            sym.name
        );
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    let removal = whole_lines(&source, sym.full_span);
    let moved_text = removal.text(&source).to_string();

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move {} out", sym.name)),
    );

    // Every file that referenced it now needs an import — including the file it
    // came from, if references remain there.
    let mut needs_import: BTreeSet<PathBuf> = BTreeSet::new();
    for reference in index.references_to(sym.id) {
        if reference.file != *destination {
            needs_import.insert(reference.file.clone());
        }
    }

    // Code that moves has to keep working where it lands: it needs whatever it
    // referenced, and it needs to be visible to the files that will now import it.
    let used = names_used_in(sym.language, &source, removal)?;
    let carried = carried_imports(index, sym, destination, &used, &source, &mut plan);
    let moved_text = if needs_import.is_empty() {
        moved_text
    } else {
        exported(sym.language, &moved_text)
    };
    // The imports the moved code needs go where imports go, not immediately above the
    // code. Prepending them to the moved text put an `import` statement in the middle
    // of the file — legal in Python and a syntax error in half the other targets, and
    // wrong-looking in all of them.
    if !carried.is_empty() {
        let existing = crate::vfs::read_to_string(destination).unwrap_or_default();
        let at = import_insertion_point_for(index, destination, &existing);
        plan.edits.add(
            destination.to_path_buf(),
            Edit::new(
                Span::new(at, at),
                carried.clone(),
                format!("what {} needs where it lands", sym.name),
            ),
        );
    }
    append_to_destination(
        &mut plan.edits,
        destination,
        &moved_text,
        format!("move {} in", sym.name),
    );

    // The destination may already have been importing it from here. It is local now,
    // so that import points at a file which no longer defines the name — and the file
    // fails on the line that used to make it work. Nothing was adding this import, so
    // nothing was removing it either.
    if let Some(edit) = drop_local_import(index, destination, &sym.file, &sym.name)? {
        plan.edits.add(destination.to_path_buf(), edit);
    }

    for file in &needs_import {
        let statement = import_statement(sym.language, file, destination, &sym.name)?;
        let target_source = crate::vfs::read_to_string(file).unwrap_or_default();
        let insert = import_insertion_point_for(index, file, &target_source);
        plan.edits.add(
            file.clone(),
            Edit::new(
                Span::new(insert, insert),
                statement,
                format!("import {} from its new home", sym.name),
            ),
        );
        plan.imports_added.push(file.clone());
    }

    Ok(plan)
}

/// Remove the destination's import of a name it is about to define itself.
///
/// Narrowed rather than deleted where the statement brings in more than one name: the
/// others are still over there and still needed.
fn drop_local_import(
    index: &Index,
    destination: &Path,
    from: &Path,
    name: &str,
) -> Result<Option<Edit>> {
    let Some(info) = index.file(destination) else {
        return Ok(None);
    };
    let Ok(source) = crate::vfs::read_to_string(destination) else {
        return Ok(None);
    };
    for import in import_statements(&info.imports) {
        if !import.names.iter().any(|n| n.local == name) {
            continue;
        }
        // Only the import that names the file the symbol is leaving. A same-named
        // thing imported from somewhere else is somebody else's.
        let points_here = repoint(&import.path, destination, from)
            .map(|p| p == import.path)
            .unwrap_or(false)
            || import.path.ends_with(&stem(from));
        if !points_here {
            continue;
        }
        let kept: Vec<&crate::model::ImportedName> =
            import.names.iter().filter(|n| n.local != name).collect();
        let replacement = match kept.is_empty() {
            true => String::new(),
            false => {
                let spelled: Vec<String> = kept
                    .iter()
                    .map(|n| n.span.text(&source).to_string())
                    .collect();
                match info.language {
                    Language::Python => {
                        format!("from {} import {}\n", import.path, spelled.join(", "))
                    }
                    _ => format!(
                        "import {{ {} }} from '{}';\n",
                        spelled.join(", "),
                        import.path
                    ),
                }
            }
        };
        let span = whole_lines(&source, import.span);
        return Ok(Some(Edit::new(
            span,
            replacement,
            format!("`{name}` is defined here now"),
        )));
    }
    Ok(None)
}

/// A file's name without its extension, for matching a relative import against it.
fn stem(file: &Path) -> String {
    file.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Every identifier the moved region names.
///
/// Deciding what the moved code depends on needs the names it mentions, and reading
/// them off the tree rather than the text keeps strings and comments out of it.
/// Locally-declared names come along too, which is harmless: they are only ever
/// matched against imports and the names the source file defines, and a local that
/// shadows one of those was already a hazard before the move.
fn names_used_in(
    language: Language,
    source: &str,
    region: Span,
) -> Result<std::collections::HashSet<String>> {
    let parsed = crate::parse::Parsers::new().parse(language, source)?;
    let mut names = std::collections::HashSet::new();
    let mut stack = vec![parsed.root()];
    while let Some(node) = stack.pop() {
        let span = Span::from(node);
        if span.end <= region.start || span.start >= region.end {
            continue;
        }
        if node.kind().ends_with("identifier") {
            names.insert(span.text(source).to_string());
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    Ok(names)
}

/// The imports the moved text needs at the top of its new file.
///
/// Two kinds. An import the source file already had, whose binding the moved code
/// uses, is re-pointed at the destination and copied across. A symbol the *source
/// file itself* defines and the moved code still calls needs a new import pointing
/// back at the source — and that symbol has to be exported for it to resolve, which
/// is done here rather than left as a note.
///
/// The import pointing back is a cycle when the source also imports the moved
/// symbol. That is legal in both languages and common in TypeScript, but Python
/// resolves imports at run time and can deadlock on one, so it is reported.
fn carried_imports(
    index: &Index,
    sym: &Symbol,
    destination: &Path,
    used: &std::collections::HashSet<String>,
    source: &str,
    plan: &mut MovePlan,
) -> String {
    let Some(info) = index.file(&sym.file) else {
        return String::new();
    };
    let mut statements: Vec<String> = Vec::new();

    for statement in import_statements(&info.imports) {
        // `import os` binds `os` without naming it anywhere in the statement, and the
        // moved code reaches `os.path.basename` through exactly that binding. Reading
        // it from the path is the same rule import liveness already uses.
        let implicit = crate::refactor::imports::implicit_binding(&statement.path, sym.language);
        // `from __future__ import annotations` binds nothing and changes how every
        // annotation in the file is evaluated. Code written under it — requests uses
        // `str | None`, which needs it below Python 3.10 — stops working the moment
        // it lands in a file without it.
        let governs_the_file = statement.path == "__future__";
        let binds_something_used = governs_the_file
            || statement.names.iter().any(|n| used.contains(&n.local))
            || statement.alias.as_ref().is_some_and(|a| used.contains(a))
            || implicit.as_deref().is_some_and(|b| used.contains(b));
        if !binds_something_used {
            continue;
        }
        match repoint(&statement.path, &sym.file, destination) {
            Some(path) => statements.push(narrowed_import(
                sym.language,
                &statement,
                used,
                &path,
                source,
            )),
            None => plan.warnings.push(format!(
                "the moved code uses `{}`, imported from `{}`; that path could not be \
                 re-expressed from the new file and was not carried over",
                statement
                    .names
                    .iter()
                    .map(|n| n.local.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                statement.path
            )),
        }
    }

    // Names the source file defines that the moved code still needs.
    let mut wanted: Vec<&crate::model::Symbol> = info
        .symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.id != sym.id && s.container.is_none() && used.contains(&s.name))
        .collect();
    wanted.sort_by(|a, b| a.name.cmp(&b.name));
    wanted.dedup_by(|a, b| a.name == b.name);

    if !wanted.is_empty() {
        let names: Vec<String> = wanted.iter().map(|s| s.name.clone()).collect();
        match back_import(sym.language, destination, &sym.file, &names) {
            Some(statement) => {
                statements.push(statement);
                for symbol in &wanted {
                    if let Some(edit) = export_edit(sym.language, &sym.file, symbol) {
                        plan.edits.add(sym.file.clone(), edit);
                    }
                }
                if sym.language == Language::Python {
                    plan.warnings.push(format!(
                        "{} now imports {} back from {}, which imports the moved symbol \
                         in turn; Python resolves that cycle at run time and may fail on it",
                        destination.display(),
                        names.join(", "),
                        sym.file.display()
                    ));
                }
            }
            None => plan.warnings.push(format!(
                "the moved code uses {} from {}, and no import path back to it could be \
                 written from the new file",
                names.join(", "),
                sym.file.display()
            )),
        }
    }

    if statements.is_empty() {
        return String::new();
    }
    // `__future__` must be the first statement in the file, so it leads whatever
    // else came along.
    statements.sort_by_key(|s| !s.contains("__future__"));
    format!("{}\n", statements.join(""))
}

/// The same module, named from the destination instead of the source.
///
/// A bare package name means the same thing from anywhere and is returned unchanged;
/// a relative path has to be recomputed, since the two files sit in different
/// directories.
fn repoint(path: &str, from: &Path, to: &Path) -> Option<String> {
    if !path.starts_with('.') {
        return Some(path.to_string());
    }
    let from_dir = from.parent()?;
    let target = normalise(&from_dir.join(path.trim_start_matches("./")));
    let stem_holder = target.clone();
    relative_module(to, &stem_holder.with_extension("ts")).or_else(|| Some(path.to_string()))
}

/// One import statement, with every name it binds.
///
/// The index records a separate [`crate::model::Import`] per imported name, each
/// carrying the span of the whole statement it came from. Iterating that list
/// directly makes a four-name import look like four statements, so anything
/// rewriting statements has to regroup them first.
struct ImportStatement {
    path: String,
    alias: Option<String>,
    names: Vec<crate::model::ImportedName>,
    span: Span,
}

fn import_statements(imports: &[crate::model::Import]) -> Vec<ImportStatement> {
    let mut grouped: Vec<ImportStatement> = Vec::new();
    for import in imports {
        match grouped.iter_mut().find(|g| g.span == import.span) {
            Some(existing) => {
                for name in &import.names {
                    if !existing.names.iter().any(|n| n.span == name.span) {
                        existing.names.push(name.clone());
                    }
                }
                if existing.alias.is_none() {
                    existing.alias = import.alias.clone();
                }
            }
            None => grouped.push(ImportStatement {
                path: import.path.clone(),
                alias: import.alias.clone(),
                names: import.names.clone(),
                span: import.span,
            }),
        }
    }
    for group in &mut grouped {
        group.names.sort_by_key(|n| n.span.start);
    }
    grouped
}

/// The import as the destination needs it: the new path, and only the names the
/// moved code actually uses.
///
/// Copying the statement whole would carry names the moved code never mentions, and
/// an unused import is an error under `noUnusedLocals` — a move should not hand back
/// a file that fails the build for a reason it introduced. A default or namespace
/// import binds one name and has no list to narrow, so it is copied as written.
fn narrowed_import(
    language: Language,
    import: &ImportStatement,
    used: &std::collections::HashSet<String>,
    path: &str,
    source: &str,
) -> String {
    let statement = import.span.text(source);
    let rewritten = |text: &str| {
        let replaced = text.replacen(&import.path, path, 1);
        if replaced.ends_with('\n') {
            replaced
        } else {
            format!("{replaced}\n")
        }
    };

    let keep: Vec<&crate::model::ImportedName> = import
        .names
        .iter()
        .filter(|n| used.contains(&n.local))
        .collect();
    if keep.is_empty() || keep.len() == import.names.len() {
        return rewritten(statement);
    }

    // Each name keeps whatever it was written with — an aliased name keeps its
    // alias, and a TypeScript `type` modifier keeps the modifier.
    let spelled: Vec<String> = keep
        .iter()
        .map(|n| {
            let written = n.span.text(source);
            let before = source[..n.span.start].trim_end();
            if before.ends_with("type") && !written.starts_with("type") {
                format!("type {written}")
            } else {
                written.to_string()
            }
        })
        .collect();

    match language {
        Language::Python => format!("from {path} import {}\n", spelled.join(", ")),
        _ => format!("import {{ {} }} from '{path}';\n", spelled.join(", ")),
    }
}

/// An import in the destination naming symbols left behind in the source file.
fn back_import(language: Language, from: &Path, to: &Path, names: &[String]) -> Option<String> {
    let joined = names.join(", ");
    Some(match language {
        Language::TypeScript | Language::Tsx => {
            format!(
                "import {{ {joined} }} from '{}';\n",
                relative_module(from, to)?
            )
        }
        Language::Python => {
            format!(
                "from {} import {joined}\n",
                python_relative_module(from, to)?
            )
        }
        _ => return None,
    })
}

/// Make a symbol visible outside its file, if the language says so and it is not
/// already.
///
/// The edit rewrites the declaration's first word rather than inserting `export`
/// ahead of it. An insertion has no width, and a file whose first line is the
/// declaration would put it at the same offset as the new import — two zero-width
/// edits at one position, whose order decides whether the result reads
/// `import …` then `export function`, or the nonsense `export import …`.
fn export_edit(language: Language, file: &Path, symbol: &Symbol) -> Option<Edit> {
    if !matches!(language, Language::TypeScript | Language::Tsx) {
        return None;
    }
    let source = crate::vfs::read_to_string(file).ok()?;
    let line_start = whole_lines(&source, symbol.full_span).start;
    let rest = &source[line_start..];
    let lead = rest.len() - rest.trim_start().len();
    let start = line_start + lead;
    let word_len = source[start..]
        .find(|c: char| c.is_whitespace())
        .unwrap_or(source.len() - start);
    let word = &source[start..start + word_len];
    if word == "export" {
        return None;
    }
    Some(Edit::new(
        Span::new(start, start + word_len),
        format!("export {word}"),
        format!("export {} so the moved code can import it", symbol.name),
    ))
}

/// The moved text, exported.
fn exported(language: Language, text: &str) -> String {
    if !matches!(language, Language::TypeScript | Language::Tsx) {
        return text.to_string();
    }
    let body = text.trim_start_matches(['\n', '\r']);
    if body.trim_start().starts_with("export") {
        return text.to_string();
    }
    let lead = &text[..text.len() - body.len()];
    format!("{lead}export {body}")
}

/// The import statement `from` needs in order to see `name` defined in `to`.
///
/// Failing to work one out is an error, not something to skip: the reference in
/// `from` is what makes the import necessary, and a move that drops it leaves code
/// that no longer compiles while reporting success.
fn import_statement(language: Language, from: &Path, to: &Path, name: &str) -> Result<String> {
    let unresolvable = || {
        anyhow::anyhow!(
            "cannot express {} as a module path from {}, so the import {} needs after \
             the move cannot be written",
            to.display(),
            from.display(),
            name
        )
    };
    Ok(match language {
        Language::TypeScript | Language::Tsx => {
            let module = relative_module(from, to).ok_or_else(unresolvable)?;
            format!("import {{ {name} }} from '{module}';\n")
        }
        // Python spells a relative module with dots, not slashes. A slash-shaped path
        // is a syntax error, which the reparse check would reject — so the move would
        // never commit at all.
        Language::Python => {
            let module = python_relative_module(from, to).ok_or_else(unresolvable)?;
            format!("from {module} import {name}\n")
        }
        other => bail!(
            "{other} does not move by relative import, so no import statement can be \
             written for {name}"
        ),
    })
}

/// A Python relative module path for `to`, as seen from `from`.
///
/// `.sibling` for a file beside it, `.sub.mod` for one below, `..up.mod` for one
/// reached by going up first — one leading dot per level, as the language spells it.
fn python_relative_module(from: &Path, to: &Path) -> Option<String> {
    let from_dir = from.parent()?;
    let to_dir = to.parent()?;
    let stem = to.file_stem()?.to_str()?;

    let mut ups = 0;
    let mut probe = from_dir;
    loop {
        if let Ok(rest) = to_dir.strip_prefix(probe) {
            let mut module = ".".repeat(ups + 1);
            for part in rest.components() {
                module.push_str(part.as_os_str().to_str()?);
                module.push('.');
            }
            module.push_str(stem);
            return Some(module);
        }
        probe = probe.parent()?;
        ups += 1;
        if ups > 16 {
            return None;
        }
    }
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
/// Where a new import goes, using the import statements the index already parsed.
///
/// The line-based fallback below cannot see a statement that spans lines: given
/// `from typing import (` on one line, `    Any,` on the next and `)` on a third,
/// it stops at `Any,` — the first line that is not itself an import — and inserts
/// the new statement *inside the parentheses*. requests writes its typing imports
/// exactly that way, so every move out of `utils.py` produced a file that would not
/// parse. The index knows where each statement ends; ask it.
fn import_insertion_point_for(index: &Index, file: &Path, source: &str) -> usize {
    let Some(info) = index.file(file) else {
        return import_insertion_point(source);
    };
    let Some(end) = info.imports.iter().map(|i| i.span.end).max() else {
        return import_insertion_point(source);
    };
    // Just past the line the last import ends on.
    match source[end..].find('\n') {
        Some(offset) => end + offset + 1,
        None => source.len(),
    }
}

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

// ---------------------------------------------------------------------------
// Rust: resolution follows module paths.
// ---------------------------------------------------------------------------

/// A Rust file's position in a crate: the `src` directory it lives under and the
/// module path from the crate root, empty for the root itself.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CrateModule {
    src: PathBuf,
    path: Vec<String>,
}

impl CrateModule {
    /// The `use` prefix naming this module from anywhere in the crate.
    fn use_prefix(&self) -> String {
        let mut out = String::from("crate");
        for segment in &self.path {
            out.push_str("::");
            out.push_str(segment);
        }
        out
    }
}

fn move_rust(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    if sym.container.is_some() {
        bail!(
            "'{}' is nested inside another definition; only top-level items can be moved",
            sym.name
        );
    }

    let from_module = crate_module(&sym.file)?;
    let to_module = crate_module(destination)?;
    if from_module.src != to_module.src {
        return Err(Refusal::Unsupported {
            operation: "move to file".into(),
            language: Language::Rust,
            because: format!(
                "{} and {} are under different crate roots ({} and {}); a move between \
                 crates needs a dependency edge this tool cannot add",
                sym.file.display(),
                destination.display(),
                from_module.src.display(),
                to_module.src.display()
            ),
        }
        .into());
    }

    if let Some(offender) = path_attribute_user(index, &to_module.src) {
        bail!(
            "{} contains a `#[path]` attribute, so a module's file location no longer \
             follows from its path; refusing to guess a `use` path for {}",
            offender.display(),
            destination.display()
        );
    }

    check_module_is_declared(&to_module, destination)?;

    let source = crate::vfs::read_to_string(&sym.file)?;
    let removal = with_rust_attributes(&source, whole_lines(&source, sym.full_span));
    let moved_text = removal.text(&source).to_string();

    // Everything that still names the item, other than the copy that travels with it.
    let outside: Vec<&crate::model::Reference> = index
        .references_to(sym.id)
        .into_iter()
        .filter(|r| !(r.file == sym.file && removal.contains(r.span)))
        .collect();

    if !sym.exported && outside.iter().any(|r| r.file != *destination) {
        bail!(
            "'{}' is private to {}; moving it into {} would put it out of reach of \
             {} use site(s). Make it `pub` (or `pub(crate)`) first.",
            sym.name,
            module_label(&from_module),
            module_label(&to_module),
            outside.iter().filter(|r| r.file != *destination).count()
        );
    }

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move {} out", sym.name)),
    );
    append_to_destination(
        &mut plan.edits,
        destination,
        &moved_text,
        format!("move {} in", sym.name),
    );

    let statement = format!("use {}::{};", to_module.use_prefix(), sym.name);

    // The destination must not keep importing what it now defines.
    if let Some(binding) = binding_import(index, destination, &sym.name) {
        drop_rust_binding(&mut plan.edits, &binding, &sym.name, destination)?;
    }

    let mut needs_use: BTreeSet<PathBuf> = BTreeSet::new();
    for reference in &outside {
        if reference.file == *destination {
            continue;
        }
        if !reference.confidence.is_safe_to_rewrite() {
            plan.warnings.push(format!(
                "{}: '{}' resolves only '{}' here, so no `use` was written for it",
                location(&reference.file, reference.span.start),
                sym.name,
                reference.confidence.as_str()
            ));
            continue;
        }
        needs_use.insert(reference.file.clone());
    }

    for file in &needs_use {
        match binding_import(index, file, &sym.name) {
            Some(binding) => {
                repoint_rust_binding(&mut plan.edits, &binding, &sym.name, &statement)?
            }
            None => {
                let target_source = crate::vfs::read_to_string(file).unwrap_or_default();
                let at = rust_use_insertion_point(&target_source);
                plan.edits.add(
                    file.clone(),
                    Edit::new(
                        Span::new(at, at),
                        format!("{statement}\n"),
                        format!("import {} from its new home", sym.name),
                    ),
                );
            }
        }
        plan.imports_added.push(file.clone());
    }

    warn_about_carried_imports(index, sym, destination, removal, &source, &mut plan);
    carry_defined_dependencies(index, sym, destination, removal, &source, &mut plan);
    Ok(plan)
}

/// Where a Rust file sits in its crate.
///
/// Refuses rather than guessing: a wrong `use` path produces a file that does not
/// compile, which is worse than declining the move.
fn crate_module(file: &Path) -> Result<CrateModule> {
    let mut src: Option<PathBuf> = None;
    for ancestor in file.ancestors().skip(1) {
        if ancestor.file_name().and_then(|n| n.to_str()) == Some("src") {
            src = Some(ancestor.to_path_buf());
            break;
        }
    }
    let Some(src) = src else {
        return Err(Refusal::Unsupported {
            operation: "move to file".into(),
            language: Language::Rust,
            because: format!(
                "{} is not under a `src/` directory, so its module path cannot be \
                 derived from its location",
                file.display()
            ),
        }
        .into());
    };

    if !crate::vfs::exists(src.join("lib.rs")) && !crate::vfs::exists(src.join("main.rs")) {
        return Err(Refusal::Unsupported {
            operation: "move to file".into(),
            language: Language::Rust,
            because: format!(
                "{} has neither lib.rs nor main.rs, so there is no crate root to anchor \
                 a `use crate::…` path to",
                src.display()
            ),
        }
        .into());
    }

    let relative = file
        .strip_prefix(&src)
        .map_err(|_| anyhow::anyhow!("{} is not under {}", file.display(), src.display()))?;

    let mut path: Vec<String> = Vec::new();
    for component in relative.components() {
        let part = component
            .as_os_str()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF-8 path component in {}", file.display()))?;
        path.push(part.to_string());
    }
    // The last component is the file; strip its extension and fold away the module
    // spellings that name their parent directory rather than a module of their own.
    if let Some(last) = path.pop() {
        let stem = last.strip_suffix(".rs").unwrap_or(&last).to_string();
        if !matches!(stem.as_str(), "mod" | "lib" | "main") {
            path.push(stem);
        }
    }
    Ok(CrateModule { src, path })
}

fn module_label(module: &CrateModule) -> String {
    if module.path.is_empty() {
        "the crate root".to_string()
    } else {
        module.use_prefix()
    }
}

/// Is every module on the path to `module` declared with a `mod` statement?
fn check_module_is_declared(module: &CrateModule, destination: &Path) -> Result<()> {
    let root = if crate::vfs::exists(module.src.join("lib.rs")) {
        module.src.join("lib.rs")
    } else {
        module.src.join("main.rs")
    };

    let mut parent_file = root;
    let mut walked: Vec<String> = Vec::new();
    for segment in &module.path {
        let source = crate::vfs::read_to_string(&parent_file).unwrap_or_default();
        if !declares_module(&source, segment) {
            bail!(
                "{} does not declare `mod {};`, so {} is not part of the module tree and \
                 a `use` path to it would not compile",
                parent_file.display(),
                segment,
                destination.display()
            );
        }
        walked.push(segment.clone());
        let dir: PathBuf = module.src.join(walked.join("/"));
        let flat = module.src.join(format!("{}.rs", walked.join("/")));
        parent_file = if crate::vfs::exists(&flat) {
            flat
        } else {
            dir.join("mod.rs")
        };
    }
    Ok(())
}

/// Does `source` contain a `mod <name>;` declaration at any nesting?
///
/// Only the `;` form is accepted: `mod name { … }` declares an inline module that is
/// not this file, so a file-derived path would be wrong.
fn declares_module(source: &str, name: &str) -> bool {
    source.lines().any(|line| {
        let mut rest = line.trim_start();
        if let Some(after) = rest.strip_prefix("pub") {
            rest = after.trim_start();
            if rest.starts_with('(') {
                match rest.find(')') {
                    Some(close) => rest = rest[close + 1..].trim_start(),
                    None => return false,
                }
            }
        }
        let Some(after_mod) = rest.strip_prefix("mod ") else {
            return false;
        };
        after_mod.trim() == format!("{name};")
    })
}

/// The first file under `src` that uses `#[path]`, which unhooks module paths from
/// file locations.
fn path_attribute_user(index: &Index, src: &Path) -> Option<PathBuf> {
    let mut found: Option<PathBuf> = None;
    for (path, info) in index.files() {
        if info.language != Language::Rust || !path.starts_with(src) {
            continue;
        }
        let Ok(text) = crate::vfs::read_to_string(path) else {
            continue;
        };
        if text.contains("#[path") && found.as_ref().is_none_or(|best| path < best) {
            found = Some(path.clone());
        }
    }
    found
}

/// The import in `file` that binds `name`, described by the spans a rewrite needs.
struct Binding {
    file: PathBuf,
    /// The whole `use …;` statement.
    statement: Span,
    /// The span of `name` inside a `use a::{X, Y};` list, when that is the form.
    in_list: Option<Span>,
    /// How many names the list holds.
    list_len: usize,
    is_glob: bool,
}

fn binding_import(index: &Index, file: &Path, name: &str) -> Option<Binding> {
    let info = index.file(file)?;
    let mut listed: Option<&crate::model::Import> = None;
    for import in &info.imports {
        if import.is_glob {
            continue;
        }
        if import.names.iter().any(|n| n.local == name) {
            listed = Some(import);
            break;
        }
        let tail = import.path.rsplit("::").find(|s| !s.is_empty());
        if import.names.is_empty() && (tail == Some(name) || import.alias.as_deref() == Some(name))
        {
            return Some(Binding {
                file: file.to_path_buf(),
                statement: import.span,
                in_list: None,
                list_len: 0,
                is_glob: false,
            });
        }
    }

    // `use a::{X, Y};` produces one import record per name, all sharing the statement
    // span, so how long the list is has to be counted across the group.
    let import = listed?;
    let entry = import.names.iter().find(|n| n.local == name)?;
    let list_len = info
        .imports
        .iter()
        .filter(|other| other.span == import.span)
        .map(|other| other.names.len())
        .sum();
    Some(Binding {
        file: file.to_path_buf(),
        statement: import.span,
        in_list: Some(entry.span),
        list_len,
        is_glob: false,
    })
}

/// Point an existing binding at the item's new home.
fn repoint_rust_binding(
    edits: &mut EditSet,
    binding: &Binding,
    name: &str,
    statement: &str,
) -> Result<()> {
    if binding.is_glob {
        return Ok(());
    }
    match binding.in_list {
        // `use a::{X};` and `use a::X;` are both wholly replaceable.
        None => edits.add(
            binding.file.clone(),
            Edit::new(
                binding.statement,
                statement,
                format!("repoint use of {name}"),
            ),
        ),
        Some(_) if binding.list_len == 1 => edits.add(
            binding.file.clone(),
            Edit::new(
                binding.statement,
                statement,
                format!("repoint use of {name}"),
            ),
        ),
        // One of several: take the name out of the list and add a statement beside it.
        Some(span) => {
            let source = crate::vfs::read_to_string(&binding.file)?;
            edits.add(
                binding.file.clone(),
                Edit::new(
                    list_entry_span(&source, span),
                    "",
                    format!("drop {name} from its old import list"),
                ),
            );
            edits.add(
                binding.file.clone(),
                Edit::new(
                    Span::new(binding.statement.end, binding.statement.end),
                    format!("\n{statement}"),
                    format!("import {name} from its new home"),
                ),
            );
        }
    }
    Ok(())
}

/// Remove a binding that the destination file no longer needs, because it is about to
/// define the item itself.
fn drop_rust_binding(
    edits: &mut EditSet,
    binding: &Binding,
    name: &str,
    destination: &Path,
) -> Result<()> {
    let source = crate::vfs::read_to_string(destination).unwrap_or_default();
    match binding.in_list {
        Some(span) if binding.list_len > 1 => edits.add(
            binding.file.clone(),
            Edit::new(
                list_entry_span(&source, span),
                "",
                format!("{name} is defined here now"),
            ),
        ),
        _ => edits.add(
            binding.file.clone(),
            Edit::new(
                whole_lines(&source, binding.statement),
                "",
                format!("{name} is defined here now"),
            ),
        ),
    }
    Ok(())
}

/// A name inside a `use a::{X, Y};` list, plus the comma that joins it to its
/// neighbour, so that removing it leaves a well-formed list.
fn list_entry_span(source: &str, name: Span) -> Span {
    let bytes = source.as_bytes();
    let mut end = name.end;
    while end < bytes.len() && bytes[end] == b' ' {
        end += 1;
    }
    if end < bytes.len() && bytes[end] == b',' {
        end += 1;
        while end < bytes.len() && bytes[end] == b' ' {
            end += 1;
        }
        return Span::new(name.start, end);
    }
    // Last in the list: take the preceding comma instead.
    let mut start = name.start;
    while start > 0 && (bytes[start - 1] == b' ' || bytes[start - 1] == b',') {
        start -= 1;
        if bytes[start] == b',' {
            break;
        }
    }
    Span::new(start, name.end)
}

/// Where a new `use` should go: after the file's header and any existing `use` lines.
fn rust_use_insertion_point(source: &str) -> usize {
    let mut offset = 0;
    let mut point = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_prelude = trimmed.starts_with("//!")
            || trimmed.starts_with("#![")
            || trimmed.starts_with("use ")
            || trimmed.starts_with("pub use ")
            || trimmed.starts_with("extern crate ");
        if is_prelude {
            point = offset + line.len();
        } else if !trimmed.is_empty() {
            break;
        }
        offset += line.len();
    }
    point
}

/// Widen a removal to swallow the attributes and doc comments written directly above
/// the item, which cannot legally stay behind without it.
fn with_rust_attributes(source: &str, span: Span) -> Span {
    let mut start = span.start;
    while start > 0 {
        let previous = full_line_span(source, start - 1);
        let trimmed = previous.text(source).trim_start();
        if trimmed.starts_with("#[") || trimmed.starts_with("///") || trimmed.starts_with("/**") {
            start = previous.start;
        } else {
            break;
        }
    }
    Span::new(start, span.end)
}

// ---------------------------------------------------------------------------
// Go: a package is a directory.
// ---------------------------------------------------------------------------

fn move_go(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    if sym.container.is_some() {
        bail!(
            "'{}' is nested inside another definition; only top-level declarations can \
             be moved",
            sym.name
        );
    }

    let source_dir = sym
        .file
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no directory", sym.file.display()))?;
    let dest_dir = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no directory", destination.display()))?;

    let source = crate::vfs::read_to_string(&sym.file)?;
    let removal = with_go_doc_comment(&source, whole_lines(&source, sym.full_span));
    let moved_text = removal.text(&source).to_string();

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move {} out", sym.name)),
    );

    if source_dir == dest_dir {
        // Same directory means same package: every reference already resolves, and
        // there is nothing whatsoever to update.
        let package = go_package(index, &sym.file).ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no package clause, so the package of the move is unknown",
                sym.file.display()
            )
        })?;
        if let Some(existing) = go_package(index, destination) {
            if existing != package {
                bail!(
                    "{} declares package {} but {} declares package {}; two packages \
                     cannot share a directory",
                    sym.file.display(),
                    package,
                    destination.display(),
                    existing
                );
            }
        }
        let header = if go_package(index, destination).is_none() {
            format!("package {package}\n\n")
        } else {
            String::new()
        };
        append_to_destination(
            &mut plan.edits,
            destination,
            &format!("{header}{moved_text}"),
            format!("move {} in", sym.name),
        );
        warn_about_carried_imports(index, sym, destination, removal, &source, &mut plan);
        carry_defined_dependencies(index, sym, destination, removal, &source, &mut plan);
        return Ok(plan);
    }

    // A different directory is a different package.
    if !sym.exported {
        bail!(
            "'{}' is unexported, so nothing outside {} can name it; moving it to {} \
             would make it unreachable. Capitalise it first.",
            sym.name,
            crate::vfs::describe_dir(source_dir),
            crate::vfs::describe_dir(dest_dir)
        );
    }

    let Some(package) =
        go_package(index, destination).or_else(|| go_package_of_dir(index, dest_dir))
    else {
        bail!(
            "no .go file in {} declares a package, so the qualifier every use site \
             would need is unknown",
            crate::vfs::describe_dir(dest_dir)
        );
    };
    let import_path = go_import_path(dest_dir).ok_or_else(|| {
        anyhow::anyhow!(
            "no go.mod above {}, so the import path of package {} cannot be derived",
            crate::vfs::describe_dir(dest_dir),
            package
        )
    })?;

    let outside: Vec<&crate::model::Reference> = index
        .references_to(sym.id)
        .into_iter()
        .filter(|r| !(r.file == sym.file && removal.contains(r.span)))
        .collect();

    let stray: Vec<&&crate::model::Reference> = outside
        .iter()
        .filter(|r| r.file.parent() != Some(source_dir))
        .collect();
    if !stray.is_empty() {
        let mut message = format!(
            "'{}' is used from {} file(s) outside package {}",
            sym.name,
            stray.len(),
            crate::vfs::describe_dir(source_dir)
        );
        for reference in &stray {
            message.push_str(&format!(
                "\n  {}",
                location(&reference.file, reference.span.start)
            ));
        }
        message.push_str(
            "\nRequalifying an already-qualified use cannot be verified from names alone; \
             nothing was changed.",
        );
        bail!("{message}");
    }

    let header = if go_package(index, destination).is_none() {
        format!("package {package}\n\n")
    } else {
        String::new()
    };
    append_to_destination(
        &mut plan.edits,
        destination,
        &format!("{header}{moved_text}"),
        format!("move {} in", sym.name),
    );

    let mut needs_import: BTreeSet<PathBuf> = BTreeSet::new();
    for reference in &outside {
        if !reference.confidence.is_safe_to_rewrite() {
            plan.warnings.push(format!(
                "{}: '{}' resolves only '{}' here, so it was left unqualified",
                location(&reference.file, reference.span.start),
                sym.name,
                reference.confidence.as_str()
            ));
            continue;
        }
        plan.edits.add(
            reference.file.clone(),
            Edit::new(
                Span::new(reference.span.start, reference.span.start),
                format!("{package}."),
                format!("qualify {} with its new package", sym.name),
            ),
        );
        needs_import.insert(reference.file.clone());
    }

    for file in &needs_import {
        let target_source = crate::vfs::read_to_string(file).unwrap_or_default();
        if target_source.contains(&format!("\"{import_path}\"")) {
            continue;
        }
        let at = go_import_insertion_point(index, file).ok_or_else(|| {
            anyhow::anyhow!(
                "{} has no package clause, so an import cannot be placed in it",
                file.display()
            )
        })?;
        plan.edits.add(
            file.clone(),
            Edit::new(
                Span::new(at, at),
                format!("\nimport \"{import_path}\"\n"),
                format!("import package {package}"),
            ),
        );
        plan.imports_added.push(file.clone());
    }

    warn_about_carried_imports(index, sym, destination, removal, &source, &mut plan);
    carry_defined_dependencies(index, sym, destination, removal, &source, &mut plan);
    Ok(plan)
}

/// The package a Go file declares.
fn go_package(index: &Index, file: &Path) -> Option<String> {
    let info = index.file(file)?;
    info.symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .find(|s| s.kind == SymbolKind::Module && s.language == Language::Go)
        .map(|s| s.name.clone())
}

/// The package declared by the .go files of a directory.
fn go_package_of_dir(index: &Index, dir: &Path) -> Option<String> {
    index
        .files()
        .filter(|(path, info)| info.language == Language::Go && path.parent() == Some(dir))
        .find_map(|(path, _)| go_package(index, path))
}

/// The import path of `dir`, derived from the nearest go.mod above it.
fn go_import_path(dir: &Path) -> Option<String> {
    for ancestor in dir.ancestors() {
        let manifest = ancestor.join("go.mod");
        let Ok(text) = crate::vfs::read_to_string(&manifest) else {
            continue;
        };
        let module = text.lines().find_map(|line| {
            line.trim_start()
                .strip_prefix("module ")
                .map(|rest| rest.trim().to_string())
        })?;
        let relative = dir.strip_prefix(ancestor).ok()?;
        let mut parts = vec![module];
        for component in relative.components() {
            parts.push(component.as_os_str().to_str()?.to_string());
        }
        return Some(parts.join("/"));
    }
    None
}

/// Just after the package clause, where an extra `import` declaration is always legal.
fn go_import_insertion_point(index: &Index, file: &Path) -> Option<usize> {
    let info = index.file(file)?;
    let clause = info
        .symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .find(|s| s.kind == SymbolKind::Module && s.language == Language::Go)?;
    let source = crate::vfs::read_to_string(file).ok()?;
    Some(full_line_span(&source, clause.full_span.start).end)
}

/// Widen a removal to swallow the `//` doc comment written directly above.
fn with_go_doc_comment(source: &str, span: Span) -> Span {
    let mut start = span.start;
    while start > 0 {
        let previous = full_line_span(source, start - 1);
        if previous.text(source).trim_start().starts_with("//") {
            start = previous.start;
        } else {
            break;
        }
    }
    Span::new(start, span.end)
}

// ---------------------------------------------------------------------------
// HCL / Terraform: a module is a directory.
// ---------------------------------------------------------------------------

fn move_hcl(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    let source_dir = sym.file.parent();
    if source_dir != destination.parent() {
        return Err(Refusal::Unsupported {
            operation: "move to file".into(),
            language: Language::Hcl,
            because: format!(
                "Terraform's module is the directory, so moving '{}' from {} to {} changes \
                 its module. Every address that names it, and its state address, would \
                 break; `moved` blocks or `terraform state mv` are the tools for that",
                sym.name,
                sym.file.display(),
                destination.display()
            ),
        }
        .into());
    }

    let source_ext = sym.file.extension().and_then(|e| e.to_str()).unwrap_or("");
    let dest_ext = destination
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if source_ext != dest_ext {
        bail!(
            "{} and {} are different kinds of Terraform file (.{source_ext} and \
             .{dest_ext}); Terraform loads them by different rules",
            sym.file.display(),
            destination.display()
        );
    }
    if sym.kind == SymbolKind::Key {
        bail!(
            "'{}' is a value in a .tfvars file, not a declaration; which values file \
             Terraform loads is decided by its name and the command line, so moving it \
             between files changes whether it is applied at all",
            sym.name
        );
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    let enclosing_locals = enclosing_locals_block(index, sym);

    if sym.container.is_some() && enclosing_locals.is_none() {
        bail!(
            "'{}' is an argument of an enclosing block, not a declaration of its own; \
             only whole blocks and `locals` entries can be moved between files",
            sym.name
        );
    }

    let removal = whole_lines(&source, sym.full_span);
    let moved_text = removal.text(&source).to_string();

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move {} out", sym.name)),
    );

    match enclosing_locals {
        // A `locals` entry only means anything inside a `locals` block, so it goes into
        // the destination's block, or into one made for it.
        Some(_) => {
            let dest_source = crate::vfs::read_to_string(destination).unwrap_or_default();
            match locals_block_in(index, destination) {
                Some(block) => {
                    let close = dest_source[..block.end].rfind('}').ok_or_else(|| {
                        anyhow::anyhow!("malformed locals block in {}", destination.display())
                    })?;
                    let at = full_line_span(&dest_source, close).start;
                    plan.edits.add(
                        destination.to_path_buf(),
                        Edit::new(
                            Span::new(at, at),
                            moved_text.clone(),
                            format!("move {} in", sym.name),
                        ),
                    );
                }
                None => append_to_destination(
                    &mut plan.edits,
                    destination,
                    &format!("locals {{\n{moved_text}}}\n"),
                    format!("move {} in", sym.name),
                ),
            }
        }
        None => append_to_destination(
            &mut plan.edits,
            destination,
            &moved_text,
            format!("move {} in", sym.name),
        ),
    }

    Ok(plan)
}

/// The `locals` block a definition sits inside, if it is a `locals` entry.
fn enclosing_locals_block<'a>(index: &'a Index, sym: &Symbol) -> Option<&'a Symbol> {
    let container = index.symbol(sym.container?)?;
    (container.name == "locals" && container.kind == SymbolKind::Block).then_some(container)
}

/// The span of a `locals` block in `file`, if it has one.
fn locals_block_in(index: &Index, file: &Path) -> Option<Span> {
    let info = index.file(file)?;
    info.symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.kind == SymbolKind::Block && s.name == "locals")
        .map(|s| s.full_span)
        .next_back()
}

// ---------------------------------------------------------------------------
// CSS: names are global; reachability is what breaks.
// ---------------------------------------------------------------------------

fn move_css(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    if sym.kind == SymbolKind::Property {
        bail!(
            "'{}' is a custom property declared inside a rule, not a rule; a declaration \
             on its own is not valid at the top level of a stylesheet",
            sym.name
        );
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    let rule = widen_to_rule(&source, sym)?;
    let removal = whole_lines(&source, rule);
    let moved_text = removal.text(&source).to_string();

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move the {} rule out", sym.name)),
    );
    append_to_destination(
        &mut plan.edits,
        destination,
        &moved_text,
        format!("move the {} rule in", sym.name),
    );

    // Nothing to repoint: a CSS name is global. What can silently break is whether the
    // destination is loaded at all where the rule used to apply.
    if !imports_reach(index, &sym.file, destination) {
        plan.warnings.push(format!(
            "{} does not reach {} through any @import, so the rules moved there will \
             stop applying wherever {} was loaded. Add an @import, or move the rule to \
             a stylesheet that is already loaded.",
            sym.file.display(),
            destination.display(),
            sym.file.display()
        ));
    }

    Ok(plan)
}

/// Widen a selector to the rule it heads.
///
/// A selector is its own symbol — that is what a rename rewrites — but what a move
/// carries is the whole rule. A rule with several selectors is refused: taking one
/// selector elsewhere means duplicating the declaration block, which is a different
/// edit with different consequences.
fn widen_to_rule(source: &str, sym: &Symbol) -> Result<Span> {
    let parsed = crate::parse::Parsers::new().parse(sym.language, source)?;
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(sym.full_span.start, sym.full_span.end)
    else {
        bail!(
            "cannot locate '{}' in {} after reparsing it",
            sym.name,
            sym.file.display()
        );
    };

    let mut rule = node;
    loop {
        if matches!(rule.kind(), "rule_set" | "keyframes_statement") {
            break;
        }
        let Some(parent) = rule.parent() else {
            bail!(
                "'{}' is not part of a rule, so there is nothing to move",
                sym.name
            );
        };
        rule = parent;
    }

    if rule.parent().map(|p| p.kind()) != Some("stylesheet") {
        bail!(
            "the rule for '{}' is nested inside a {}; moving it out of that context \
             would change when it applies",
            sym.name,
            rule.parent().map(|p| p.kind()).unwrap_or("block")
        );
    }

    if rule.kind() == "rule_set" {
        let mut cursor = rule.walk();
        let selectors = rule
            .named_children(&mut cursor)
            .find(|c| c.kind() == "selectors");
        if let Some(selectors) = selectors {
            let mut inner = selectors.walk();
            let count = selectors
                .named_children(&mut inner)
                .filter(|c| !c.kind().contains("comment"))
                .count();
            if count > 1 {
                bail!(
                    "the rule for '{}' has {} selectors; moving one of them would have \
                     to duplicate the declaration block, which is a rewrite rather than \
                     a move",
                    sym.name,
                    count
                );
            }
        }
    }

    Ok(Span::from(rule))
}

/// Is `target` reachable from `origin` by following `@import`s?
fn imports_reach(index: &Index, origin: &Path, target: &Path) -> bool {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut queue = vec![origin.to_path_buf()];
    while let Some(file) = queue.pop() {
        if !seen.insert(file.clone()) {
            continue;
        }
        let Some(info) = index.file(&file) else {
            continue;
        };
        for import in &info.imports {
            let Some(dir) = file.parent() else { continue };
            let base = dir.join(import.path.trim_start_matches("./"));
            for candidate in [
                base.clone(),
                base.with_extension("css"),
                base.with_extension("scss"),
            ] {
                if candidate == target {
                    return true;
                }
                if index.file(&candidate).is_some() {
                    queue.push(candidate);
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Markdown: a section is a heading and everything under it.
// ---------------------------------------------------------------------------

fn move_markdown(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    if sym.kind != SymbolKind::Heading {
        bail!(
            "'{}' is a {}, not a heading; a Markdown move takes a section, which is a \
             heading and the content under it",
            sym.name,
            sym.kind.as_str()
        );
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    let headings = file_headings(index, &sym.file);
    let removal = section_span(&source, sym, &headings);
    let moved_text = removal.text(&source).to_string();

    // Every anchor that travelled with the section, so links to it can be repointed.
    let moved_slugs: HashSet<String> = headings
        .iter()
        .filter(|h| removal.contains(h.full_span))
        .map(|h| slug(&h.name))
        .collect();
    let staying_slugs: HashSet<String> = headings
        .iter()
        .filter(|h| !removal.contains(h.full_span))
        .map(|h| slug(&h.name))
        .collect();

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move the {} section out", sym.name)),
    );
    append_to_destination(
        &mut plan.edits,
        destination,
        &moved_text,
        format!("move the {} section in", sym.name),
    );

    // Same-document anchors in the file the section left: they now point at nothing.
    let Some(from_dir) = sym.file.parent() else {
        bail!("{} has no directory", sym.file.display());
    };
    let Some(link) = relative_link(from_dir, destination) else {
        bail!(
            "cannot express {} relative to {}",
            destination.display(),
            crate::vfs::describe_dir(from_dir)
        );
    };

    // Read the destinations from the text rather than from the index: a resolved
    // reference spans the fragment alone, and repointing one means rewriting the whole
    // destination. The cross-document pass below reads them the same way.
    if let Ok(text) = crate::vfs::read_to_string(&sym.file) {
        for span in link_destinations(&text) {
            let Some(anchor) = span.text(&text).strip_prefix('#') else {
                continue;
            };
            if removal.contains(span) {
                // It travels with the section. If its target stayed behind, the link
                // breaks in the other direction, which is the reader's to fix.
                if staying_slugs.contains(anchor) {
                    plan.warnings.push(format!(
                        "the moved section links to #{anchor}, which stays in {}; that \
                         link will not resolve from {}",
                        sym.file.display(),
                        destination.display()
                    ));
                }
                continue;
            }
            if !moved_slugs.contains(anchor) {
                continue;
            }
            plan.edits.add(
                sym.file.clone(),
                Edit::new(
                    span,
                    format!("{link}#{anchor}"),
                    format!("#{anchor} lives in {} now", destination.display()),
                ),
            );
        }
    }

    // Cross-document links written as `path/to/doc.md#anchor` elsewhere in the repo.
    let mut updated: BTreeSet<PathBuf> = BTreeSet::new();
    for (path, info) in index.files() {
        if info.language != Language::Markdown || path == &sym.file {
            continue;
        }
        let Ok(text) = crate::vfs::read_to_string(path) else {
            continue;
        };
        let Some(dir) = path.parent() else { continue };
        for span in link_destinations(&text) {
            let destination_text = span.text(&text);
            let Some((target, anchor)) = destination_text.rsplit_once('#') else {
                continue;
            };
            if target.is_empty() || !moved_slugs.contains(anchor) {
                continue;
            }
            if normalise(&dir.join(target)) != normalise(&sym.file) {
                continue;
            }
            let Some(new_link) = relative_link(dir, destination) else {
                continue;
            };
            plan.edits.add(
                path.clone(),
                Edit::new(
                    span,
                    format!("{new_link}#{anchor}"),
                    format!("#{anchor} lives in {} now", destination.display()),
                ),
            );
            updated.insert(path.clone());
        }
    }
    plan.imports_added.extend(updated);

    Ok(plan)
}

/// Every heading in a file, in source order.
fn file_headings<'a>(index: &'a Index, file: &Path) -> Vec<&'a Symbol> {
    let Some(info) = index.file(file) else {
        return Vec::new();
    };
    let mut headings: Vec<&Symbol> = info
        .symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.kind == SymbolKind::Heading)
        .collect();
    headings.sort_by_key(|s| s.full_span.start);
    headings
}

/// A heading and the content under it, up to the next heading of the same or higher
/// level — which is exactly what a reader means by "this section".
fn section_span(source: &str, heading: &Symbol, headings: &[&Symbol]) -> Span {
    let level = heading_level(source, heading);
    let start = full_line_span(source, heading.full_span.start).start;
    let mut end = source.len();
    for other in headings {
        if other.full_span.start <= heading.full_span.start {
            continue;
        }
        if heading_level(source, other) <= level {
            end = full_line_span(source, other.full_span.start).start;
            break;
        }
    }
    Span::new(start, end.max(heading.full_span.end))
}

/// 1 for `#` and for a `===` underline, 2 for `##` and `---`, and so on.
fn heading_level(source: &str, heading: &Symbol) -> usize {
    let text = heading.full_span.text(source);
    let hashes = text.trim_start().chars().take_while(|c| *c == '#').count();
    if hashes > 0 {
        return hashes;
    }
    match text.lines().nth(1).map(|l| l.trim_start().starts_with('=')) {
        Some(true) => 1,
        _ => 2,
    }
}

/// Every `link_destination` in a Markdown document.
fn link_destinations(source: &str) -> Vec<Span> {
    let Ok(parsed) = crate::parse::Parsers::new().parse(Language::Markdown, source) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // An inline link's destination lives in an inline sub-tree; only the destination
    // of a link reference definition is in the block tree.
    let mut stack: Vec<_> = parsed.roots().collect();
    while let Some(node) = stack.pop() {
        if node.kind() == "link_destination" {
            out.push(Span::from(node));
        }
        for i in (0..node.child_count() as u32).rev() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }
    out.sort();
    out
}

/// A path for `to` as a Markdown link would write it from `from_dir`.
fn relative_link(from_dir: &Path, to: &Path) -> Option<String> {
    let mut ups = 0;
    let mut probe = from_dir;
    loop {
        if let Ok(rest) = to.strip_prefix(probe) {
            let mut parts: Vec<String> = vec!["..".to_string(); ups];
            for component in rest.components() {
                parts.push(component.as_os_str().to_str()?.to_string());
            }
            return Some(parts.join("/"));
        }
        probe = probe.parent()?;
        ups += 1;
        if ups > 16 {
            return None;
        }
    }
}

/// Resolve `.` and `..` textually, so two spellings of one path compare equal.
fn normalise(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Zig: a file is a namespace, reached through `@import`.
// ---------------------------------------------------------------------------

fn move_zig(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    if sym.container.is_some() {
        bail!(
            "'{}' is nested inside another declaration; only top-level declarations can \
             be moved",
            sym.name
        );
    }
    if let Some(existing) = zig_top_level(index, destination)
        .into_iter()
        .find(|s| s.name == sym.name)
    {
        return Err(Refusal::NameCollision {
            existing: existing.name.clone(),
            file: destination.to_path_buf(),
        }
        .into());
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    // A `///` doc comment is `//`-prefixed, so the Go widening reads it too.
    let removal = with_go_doc_comment(&source, whole_lines(&source, sym.full_span));
    let moved_text = removal.text(&source).to_string();

    let outside: Vec<&crate::model::Reference> = index
        .references_to(sym.id)
        .into_iter()
        .filter(|r| !(r.file == sym.file && removal.contains(r.span)))
        .collect();

    // Zig's `pub` is what makes a declaration visible through an `@import`. Without
    // it, everything that names the declaration today stops compiling the moment it
    // stops sharing a file with them.
    if !sym.exported {
        let stranded = outside.iter().filter(|r| r.file != *destination).count();
        if stranded > 0 {
            bail!(
                "'{}' is not `pub`, so only {} can name it; moving it to {} would put it \
                 out of reach of {} use site(s). Mark it `pub` first.",
                sym.name,
                sym.file.display(),
                destination.display(),
                stranded
            );
        }
    }

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move {} out", sym.name)),
    );
    append_to_destination(
        &mut plan.edits,
        destination,
        &moved_text,
        format!("move {} in", sym.name),
    );

    let mut by_file: BTreeMap<PathBuf, Vec<&crate::model::Reference>> = BTreeMap::new();
    for reference in &outside {
        if reference.file == *destination {
            continue;
        }
        by_file
            .entry(reference.file.clone())
            .or_default()
            .push(reference);
    }

    for (file, references) in &by_file {
        let text = crate::vfs::read_to_string(file)?;
        let parsed = crate::parse::Parsers::new().parse(Language::Zig, &text)?;

        // Whatever this file already calls the destination, or what it would have to
        // start calling it.
        let existing = zig_import_binding(index, file, destination);
        let namespace = match &existing {
            Some(local) => local.clone(),
            None => {
                let Some(path) = zig_import_path(file, destination) else {
                    return Err(Refusal::Unsupported {
                        operation: "move to file".into(),
                        language: Language::Zig,
                        because: format!(
                            "{} would have to reach {} through a relative path that climbs \
                             above its own directory. Zig refuses an `@import` that leaves \
                             the module root, and where that root is cannot be read off the \
                             two paths, so no import can be written for it",
                            file.display(),
                            destination.display()
                        ),
                    }
                    .into());
                };
                let local = zig_namespace_name(destination)?;
                if let Some(clash) = zig_top_level(index, file)
                    .into_iter()
                    .find(|s| s.name == local)
                {
                    return Err(Refusal::NameCollision {
                        existing: clash.name.clone(),
                        file: file.clone(),
                    }
                    .into());
                }
                let at = zig_import_insertion_point(&text);
                plan.edits.add(
                    file.clone(),
                    Edit::new(
                        Span::new(at, at),
                        format!("const {local} = @import(\"{path}\");\n"),
                        format!("import {} from its new home", sym.name),
                    ),
                );
                plan.imports_added.push(file.clone());
                local
            }
        };

        let mut repointed: Vec<Span> = Vec::new();
        for reference in references {
            match zig_qualifier(&parsed, reference.span) {
                // `other.thing` — the namespace it was reached through has to change.
                Some(object) => {
                    let qualifier = Span::from(object).text(&text);
                    let names_source = zig_import_binding_target(index, file, qualifier).as_deref()
                        == Some(&*sym.file);
                    if !names_source {
                        plan.warnings.push(format!(
                            "{}: `{qualifier}.{}` is not reached through an `@import` of {}, \
                             so it was left alone",
                            location(file, reference.span.start),
                            sym.name,
                            sym.file.display()
                        ));
                        continue;
                    }
                    plan.edits.add(
                        file.clone(),
                        Edit::new(
                            Span::from(object),
                            namespace.clone(),
                            format!("{} lives in {} now", sym.name, destination.display()),
                        ),
                    );
                    repointed.push(Span::from(object));
                }
                // A bare `thing` only resolves inside the file that declares it.
                None if *file == sym.file => plan.edits.add(
                    file.clone(),
                    Edit::new(
                        Span::new(reference.span.start, reference.span.start),
                        format!("{namespace}."),
                        format!("{} lives in {} now", sym.name, destination.display()),
                    ),
                ),
                None => plan.warnings.push(format!(
                    "{}: `{}` is named without a namespace in a file that does not declare \
                     it, which Zig cannot resolve either way; it was left alone",
                    location(file, reference.span.start),
                    sym.name
                )),
            }
        }

        // A namespace kept only for the declaration that left is dead weight, and Zig
        // does not complain about an unused container-level const the way it does
        // about an unused local, so nothing else would say it.
        if let Some(old) = zig_import_binding(index, file, &sym.file) {
            let still_used = index.file(file).is_some_and(|info| {
                info.references
                    .iter()
                    .map(|i| &index.references[*i])
                    .any(|r| r.name == old && !repointed.contains(&r.span))
            });
            if !still_used {
                plan.warnings.push(format!(
                    "{} imports {} as `{old}` only for `{}`; that import may now be unused",
                    file.display(),
                    sym.file.display(),
                    sym.name
                ));
            }
        }
    }

    warn_about_carried_imports(index, sym, destination, removal, &source, &mut plan);
    carry_defined_dependencies(index, sym, destination, removal, &source, &mut plan);
    Ok(plan)
}

/// Top-level declarations of a Zig file.
fn zig_top_level<'a>(index: &'a Index, file: &Path) -> Vec<&'a Symbol> {
    let Some(info) = index.file(file) else {
        return Vec::new();
    };
    info.symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.container.is_none())
        .collect()
}

/// The local name `file` binds the `@import` of `target` to, if it has one.
fn zig_import_binding(index: &Index, file: &Path, target: &Path) -> Option<String> {
    let info = index.file(file)?;
    info.imports.iter().find_map(|import| {
        (zig_import_file(file, &import.path).as_deref() == Some(target))
            .then(|| zig_import_local(import))
            .flatten()
    })
}

/// The file `local` names in `file`, if `local` binds an `@import` of a Zig source.
fn zig_import_binding_target(index: &Index, file: &Path, local: &str) -> Option<PathBuf> {
    let info = index.file(file)?;
    info.imports.iter().find_map(|import| {
        (zig_import_local(import).as_deref() == Some(local))
            .then(|| zig_import_file(file, &import.path))
            .flatten()
    })
}

fn zig_import_local(import: &crate::model::Import) -> Option<String> {
    import
        .names
        .first()
        .map(|n| n.local.clone())
        .or_else(|| import.alias.clone())
}

/// The workspace file an `@import` path names, or `None` for a package such as `std`.
fn zig_import_file(from: &Path, path: &str) -> Option<PathBuf> {
    if !path.ends_with(".zig") {
        return None;
    }
    Some(normalise(&from.parent()?.join(path)))
}

/// The path `from` would have to write to `@import` `to`.
///
/// `None` when the path would have to climb above `from`'s own directory: Zig rejects
/// an `@import` that leaves the module root, and nothing in two file paths says where
/// that root is.
fn zig_import_path(from: &Path, to: &Path) -> Option<String> {
    let rest = to.strip_prefix(from.parent()?).ok()?;
    let mut parts = Vec::new();
    for component in rest.components() {
        parts.push(component.as_os_str().to_str()?.to_string());
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The identifier a new `@import` of `destination` is bound to.
fn zig_namespace_name(destination: &Path) -> Result<String> {
    let stem = destination
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("{} has no file name", destination.display()))?;
    let mut out = String::with_capacity(stem.len());
    for ch in stem.chars() {
        out.push(if ch.is_ascii_alphanumeric() || ch == '_' {
            ch
        } else {
            '_'
        });
    }
    if out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if out.is_empty() {
        bail!(
            "{} has no file name a Zig identifier can be derived from",
            destination.display()
        );
    }
    Ok(out)
}

/// The `object` of `object.member` when `span` is the member, or `None` when the use
/// is a bare identifier.
fn zig_qualifier<'a>(
    parsed: &'a crate::parse::Parsed,
    span: Span,
) -> Option<tree_sitter::Node<'a>> {
    let node = parsed
        .root()
        .descendant_for_byte_range(span.start, span.end)?;
    let parent = node.parent()?;
    if parent.kind() != "field_expression" {
        return None;
    }
    if parent.child_by_field_name("member").map(Span::from) != Some(Span::from(node)) {
        return None;
    }
    parent.child_by_field_name("object")
}

/// Where a new `@import` goes: after the file header and any `@import`s already there.
fn zig_import_insertion_point(source: &str) -> usize {
    let mut offset = 0;
    let mut point = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_import = (trimmed.starts_with("const ") || trimmed.starts_with("pub const "))
            && trimmed.contains("@import(");
        if trimmed.starts_with("//") || is_import {
            point = offset + line.len();
        } else if !trimmed.is_empty() {
            break;
        }
        offset += line.len();
    }
    point
}

// ---------------------------------------------------------------------------
// Bash: there is no import, only `source`.
// ---------------------------------------------------------------------------

fn move_bash(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    use super::signature::{shell_reaches, shell_source_graph};

    if sym.kind != SymbolKind::Function {
        bail!(
            "'{}' is a {}; only a function can be moved between scripts. A variable's \
             value depends on when its assignment ran, so moving one changes what it \
             holds rather than where it lives",
            sym.name,
            sym.kind.as_str()
        );
    }
    if sym.container.is_some() {
        bail!(
            "'{}' is defined inside another function or subshell; only a top-level \
             function can be moved",
            sym.name
        );
    }
    if let Some(existing) = index
        .file(destination)
        .into_iter()
        .flat_map(|info| info.symbols.iter())
        .filter_map(|id| index.symbol(*id))
        .find(|s| s.name == sym.name && s.kind == SymbolKind::Function)
    {
        return Err(Refusal::NameCollision {
            existing: existing.name.clone(),
            file: destination.to_path_buf(),
        }
        .into());
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    let removal = with_shell_comment(&source, whole_lines(&source, sym.full_span));
    let moved_text = removal.text(&source).to_string();

    let (mut sources, opaque) = shell_source_graph(index);

    // Everything that still runs the name after the definition has left.
    let mut callers: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for reference in &index.references {
        if reference.language != Language::Bash
            || reference.kind != crate::model::ReferenceKind::Call
            || reference.name != sym.name
            || reference.file == *destination
        {
            continue;
        }
        if reference.file == sym.file && removal.contains(reference.span) {
            continue;
        }
        *callers.entry(reference.file.clone()).or_default() += 1;
    }

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move {} out", sym.name)),
    );
    append_to_destination(
        &mut plan.edits,
        destination,
        &moved_text,
        format!("move {} in", sym.name),
    );

    let mut sourced_anywhere = false;
    for (file, uses) in &callers {
        // A caller that computes what it sources could already have the function in
        // scope, or not; nothing in the text says which.
        if opaque.contains(file) {
            return Err(Refusal::Unknowable {
                detail: format!(
                    "{} runs `{}` and sources a path that is not a literal, so what is \
                     already in its scope cannot be known",
                    file.display(),
                    sym.name
                ),
            }
            .into());
        }
        // A caller that never sourced the defining file was never running this
        // function, so the move does not break it.
        if *file != sym.file && !shell_reaches(&sources, file, &sym.file) {
            plan.warnings.push(format!(
                "{} runs `{}` {uses} time(s) but never sources {}, so it was already \
                 calling something else and was left alone",
                file.display(),
                sym.name,
                sym.file.display()
            ));
            continue;
        }
        if shell_reaches(&sources, file, destination) {
            continue;
        }

        let Some(dir) = file.parent() else {
            bail!("{} is not inside a directory", file.display());
        };
        let Some(path) = shell_source_path(dir, destination) else {
            bail!(
                "cannot express {} relative to {}",
                destination.display(),
                crate::vfs::describe_dir(dir)
            );
        };
        let text = crate::vfs::read_to_string(file).unwrap_or_default();
        let at = shell_prelude_end(&text);
        plan.edits.add(
            file.clone(),
            Edit::new(
                Span::new(at, at),
                format!("source \"{path}\"\n"),
                format!("{} lives in {} now", sym.name, destination.display()),
            ),
        );
        plan.imports_added.push(file.clone());
        // A file that sources the destination now puts it in scope for whatever
        // sources *it*, so a later caller needs no second `source`.
        sources
            .entry(file.clone())
            .or_default()
            .push(normalise(destination));
        sourced_anywhere = true;
    }

    if sourced_anywhere {
        plan.warnings.push(format!(
            "`source` resolves a relative path against the working directory, not the \
             script's own location, so the added lines only work when the scripts are run \
             from the right directory. Consider `source \"$(dirname \"${{BASH_SOURCE[0]}}\")/{}\"`.",
            destination
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("lib.sh")
        ));
    }

    Ok(plan)
}

/// Widen a removal to swallow the `#` comment block written directly above.
fn with_shell_comment(source: &str, span: Span) -> Span {
    let mut start = span.start;
    while start > 0 {
        let previous = full_line_span(source, start - 1);
        let trimmed = previous.text(source).trim_start();
        // The shebang belongs to the file, never to the definition below it.
        if !trimmed.starts_with('#') || trimmed.starts_with("#!") {
            break;
        }
        start = previous.start;
    }
    Span::new(start, span.end)
}

/// The path a `source` in `from_dir` has to write to reach `to`.
///
/// A bare `source lib.sh` searches `$PATH`, so a same-directory target still needs an
/// explicit `./`.
fn shell_source_path(from_dir: &Path, to: &Path) -> Option<String> {
    let link = relative_link(from_dir, to)?;
    Some(if link.starts_with('.') {
        link
    } else {
        format!("./{link}")
    })
}

/// Where a `source` line goes: after the shebang, the header comments and whatever the
/// script already sources.
fn shell_prelude_end(source: &str) -> usize {
    let mut offset = 0;
    let mut point = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_prelude = trimmed.starts_with('#')
            || trimmed.starts_with("set ")
            || trimmed.starts_with("source ")
            || trimmed.starts_with(". ");
        if is_prelude {
            point = offset + line.len();
        } else if !trimmed.is_empty() {
            break;
        }
        offset += line.len();
    }
    point
}

// ---------------------------------------------------------------------------
// YAML and Helm: a values key is addressed by its path, which names no file.
// ---------------------------------------------------------------------------

fn move_values_key(index: &Index, sym: &Symbol, destination: &Path) -> Result<MovePlan> {
    if sym.kind != SymbolKind::Key {
        bail!(
            "'{}' is a {}; only a mapping key and the subtree under it can move between \
             values files. An anchor is resolved within one document and does not \
             survive leaving it",
            sym.name,
            sym.kind.as_str()
        );
    }
    if let Some(container) = sym.container.and_then(|id| index.symbol(id)) {
        bail!(
            "'{}' is nested under `{}`, so its address is the whole path down to it. \
             Appending it to the top level of {} would change that path and every \
             reference with it; only a top-level key can be moved",
            sym.name,
            container.name,
            destination.display()
        );
    }

    let source = crate::vfs::read_to_string(&sym.file)?;
    let indent = line_indent(&source, sym.full_span.start);
    if !indent.is_empty() {
        bail!(
            "'{}' at {} is indented by {} column(s) although nothing encloses it, so what \
             mapping it belongs to cannot be told from the text; refusing to guess an \
             indentation for it in {}",
            sym.name,
            location(&sym.file, sym.full_span.start),
            indent.len(),
            destination.display()
        );
    }

    // Appending to a file that holds several documents would land the key in the last
    // one, which is a choice this tool has no way to make.
    for file in [&sym.file, &destination.to_path_buf()] {
        let text = crate::vfs::read_to_string(file).unwrap_or_default();
        let language = crate::lang::detect(file).unwrap_or(sym.language);
        if yaml_document_count(language, &text)? > 1 {
            bail!(
                "{} holds more than one document; which one a top-level key belongs to is \
                 not decidable from the key alone",
                file.display()
            );
        }
    }

    if let Some(existing) = index
        .file(destination)
        .into_iter()
        .flat_map(|info| info.symbols.iter())
        .filter_map(|id| index.symbol(*id))
        .find(|s| s.container.is_none() && s.kind == SymbolKind::Key && s.name == sym.name)
    {
        return Err(Refusal::NameCollision {
            existing: existing.name.clone(),
            file: destination.to_path_buf(),
        }
        .into());
    }

    // The destination's own top level must really be at column zero, or a key appended
    // there joins nothing.
    let destination_source = crate::vfs::read_to_string(destination).unwrap_or_default();
    for key in index
        .file(destination)
        .into_iter()
        .flat_map(|info| info.symbols.iter())
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.container.is_none() && s.kind == SymbolKind::Key)
    {
        let existing_indent = line_indent(&destination_source, key.full_span.start);
        if !existing_indent.is_empty() {
            bail!(
                "the top-level keys of {} are indented by {} column(s), so a key appended \
                 at column zero would not join the same mapping",
                destination.display(),
                existing_indent.len()
            );
        }
    }

    let removal = with_yaml_comment(&source, whole_lines(&source, sym.full_span));
    let moved_text = removal.text(&source).to_string();

    let mut plan = MovePlan::new(sym, destination);
    plan.edits.add(
        sym.file.clone(),
        Edit::new(removal, "", format!("move `{}` out", sym.name)),
    );
    append_to_destination(
        &mut plan.edits,
        destination,
        &moved_text,
        format!("move `{}` in", sym.name),
    );

    // Nothing to repoint: `.Values.<key>` names a path, and a top-level key's path is
    // the same in every values file. What changes is whether the file is read at all.
    let file_name = sym
        .file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if file_name.eq_ignore_ascii_case("values.yaml") {
        plan.warnings.push(format!(
            "`helm install` reads only values.yaml by default, so `{}` will be unset \
             unless {} is passed with `-f`",
            sym.name,
            destination.display()
        ));
    }
    if sym.file.parent() != destination.parent() {
        plan.warnings.push(format!(
            "{} is not in the same directory as {}; check that whatever loads the values \
             still finds it",
            destination.display(),
            sym.file.display()
        ));
    }

    Ok(plan)
}

/// How many documents a YAML file holds.
fn yaml_document_count(language: Language, source: &str) -> Result<usize> {
    if source.trim().is_empty() {
        return Ok(0);
    }
    let parsed = crate::parse::Parsers::new().parse(language, source)?;
    let root = parsed.root();
    let mut cursor = root.walk();
    let count = root
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "document")
        .count();
    Ok(count)
}

/// Widen a removal to swallow the `#` comment block written directly above the key.
///
/// A comment block that opens the file describes the file, not the first key in it, so
/// it stays behind.
fn with_yaml_comment(source: &str, span: Span) -> Span {
    let mut start = span.start;
    while start > 0 {
        let previous = full_line_span(source, start - 1);
        if !previous.text(source).trim_start().starts_with('#') || previous.start == 0 {
            break;
        }
        start = previous.start;
    }
    Span::new(start, span.end)
}

// ---------------------------------------------------------------------------
// Shared machinery.
// ---------------------------------------------------------------------------

/// Take the whole line(s) a definition sits on, so the moved text carries its own
/// formatting and the hole left behind does not become a stray blank line.
fn whole_lines(source: &str, span: Span) -> Span {
    let start_line = full_line_span(source, span.start);
    let end_line = full_line_span(source, span.end.saturating_sub(1));
    Span::new(start_line.start, end_line.end.max(span.end))
}

/// Append `text` to a file that may not exist yet.
fn append_to_destination(
    edits: &mut EditSet,
    destination: &Path,
    text: &str,
    reason: impl Into<String>,
) {
    let existing = crate::vfs::read_to_string(destination).unwrap_or_default();
    let separator = if existing.is_empty() || existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    edits.add(
        destination.to_path_buf(),
        Edit::new(
            Span::new(existing.len(), existing.len()),
            format!("{separator}{text}"),
            reason,
        ),
    );
}

/// Report the imports the moved text depended on, in both directions.
///
/// A move takes code away from the imports that fed it. Nothing here is edited: which
/// import a name came from is exactly the question this index answers only weakly for
/// Rust and Go, and a wrong import edit breaks the build silently.
fn warn_about_carried_imports(
    index: &Index,
    sym: &Symbol,
    destination: &Path,
    removal: Span,
    source: &str,
    plan: &mut MovePlan,
) {
    let Some(info) = index.file(&sym.file) else {
        return;
    };
    let moved_text = removal.text(source);

    for import in &info.imports {
        let bound: Vec<String> = if !import.names.is_empty() {
            import.names.iter().map(|n| n.local.clone()).collect()
        } else if let Some(alias) = &import.alias {
            vec![alias.clone()]
        } else {
            import
                .path
                .rsplit(['/', ':'])
                .find(|s| !s.is_empty())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default()
        };

        for name in bound {
            if !mentions_word(moved_text, &name) {
                continue;
            }
            let still_used = info
                .references
                .iter()
                .map(|i| &index.references[*i])
                .any(|r| r.name == name && !removal.contains(r.span));
            if !still_used {
                plan.warnings.push(format!(
                    "{} imports '{}' only for the code being moved; the import may now \
                     be unused",
                    sym.file.display(),
                    import.path
                ));
            }
            let destination_has = index
                .file(destination)
                .is_some_and(|d| d.imports.iter().any(|i| i.path == import.path));
            if !destination_has {
                plan.warnings.push(format!(
                    "the moved code uses '{}' from '{}', which {} does not import",
                    name,
                    import.path,
                    destination.display()
                ));
            }
        }
    }
}

/// Names the source file *defines* that the moved code still uses.
///
/// The generic path writes an import pointing back for these. The per-language paths
/// looked only at what the source file imported, never at what it declared — so a Rust
/// function that used a `const` beside it landed in a file where that name means
/// nothing, `cargo check` said `cannot find value PI in this scope`, and `fr move` said
/// nothing at all.
///
/// Rust gets the import written, because the module path is already derived here for
/// the move in the other direction. The rest get told, which is the least this can do
/// and infinitely more than it did.
fn carry_defined_dependencies(
    index: &Index,
    sym: &Symbol,
    destination: &Path,
    removal: Span,
    source: &str,
    plan: &mut MovePlan,
) {
    // A Go move inside one package keeps one scope: nothing has to be named again.
    if sym.language == Language::Go && sym.file.parent() == destination.parent() {
        return;
    }
    let Some(info) = index.file(&sym.file) else {
        return;
    };
    let Ok(used) = names_used_in(sym.language, source, removal) else {
        return;
    };

    let mut wanted: Vec<&Symbol> = info
        .symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| s.id != sym.id && s.container.is_none() && used.contains(&s.name))
        .collect();
    wanted.sort_by(|a, b| a.name.cmp(&b.name));
    wanted.dedup_by(|a, b| a.name == b.name);
    if wanted.is_empty() {
        return;
    }
    let names: Vec<String> = wanted.iter().map(|s| s.name.clone()).collect();

    if sym.language == Language::Rust {
        if let (Ok(from), Ok(_)) = (crate_module(&sym.file), crate_module(destination)) {
            let joined = match names.len() {
                1 => names[0].clone(),
                _ => format!("{{{}}}", names.join(", ")),
            };
            let existing = crate::vfs::read_to_string(destination).unwrap_or_default();
            let at = rust_use_insertion_point(&existing);
            plan.edits.add(
                destination.to_path_buf(),
                Edit::new(
                    Span::new(at, at),
                    format!("use {}::{joined};\n", from.use_prefix()),
                    format!("what {} needs where it lands", sym.name),
                ),
            );
            // A private item is invisible from another module, so the `use` alone
            // would not compile. The generic path exports for the same reason.
            for symbol in &wanted {
                if let Some(edit) = rust_pub_edit(&sym.file, symbol) {
                    plan.edits.add(sym.file.clone(), edit);
                }
            }
            return;
        }
    }

    plan.warnings.push(format!(
        "the moved code uses {} defined in {}, and no import naming {} from {} could be \
         written; add it by hand or the moved code will not compile",
        names.join(", "),
        sym.file.display(),
        if names.len() == 1 { "it" } else { "them" },
        destination.display()
    ));
}

/// Make a Rust item visible outside its module, if it is not already.
///
/// The same rewrite-the-first-word shape [`export_edit`] uses, and for the same reason:
/// a zero-width insertion at the start of a file collides with the new `use`.
fn rust_pub_edit(file: &Path, symbol: &Symbol) -> Option<Edit> {
    let source = crate::vfs::read_to_string(file).ok()?;
    let line_start = whole_lines(&source, symbol.full_span).start;
    let rest = &source[line_start..];
    let lead = rest.len() - rest.trim_start().len();
    let start = line_start + lead;
    let word_len = source[start..]
        .find(|c: char| c.is_whitespace())
        .unwrap_or(source.len() - start);
    let word = &source[start..start + word_len];
    if word == "pub" || word.starts_with("pub(") {
        return None;
    }
    Some(Edit::new(
        Span::new(start, start + word_len),
        format!("pub {word}"),
        format!("make {} visible from its new caller", symbol.name),
    ))
}

/// Does `text` contain `word` as a whole identifier?
fn mentions_word(text: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let boundary = |c: char| !c.is_alphanumeric() && c != '_';
    let mut from = 0;
    while let Some(found) = text[from..].find(word) {
        let start = from + found;
        let end = start + word.len();
        let before_ok = start == 0 || text[..start].chars().next_back().is_some_and(boundary);
        let after_ok = end == text.len() || text[end..].chars().next().is_some_and(boundary);
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

/// `path:line:col`, for warnings that point a human at a file.
fn location(file: &Path, offset: usize) -> String {
    let Ok(source) = crate::vfs::read_to_string(file) else {
        return file.display().to_string();
    };
    let position = LineIndex::new(&source).line_col(offset, &source);
    format!("{}:{}", file.display(), position)
}

/// Symbols eligible to be moved, by what a move means in each language.
pub fn movable(index: &Index, file: &Path) -> Vec<SymbolId> {
    let Some(info) = index.file(file) else {
        return Vec::new();
    };
    info.symbols
        .iter()
        .filter_map(|id| index.symbol(*id))
        .filter(|s| match s.language {
            // Nothing, because `move` refuses for Java: offering a symbol the
            // operation will then decline is worse than an empty list.
            Language::Java => false,
            Language::TypeScript | Language::Tsx | Language::Python => {
                s.container.is_none()
                    && matches!(
                        s.kind,
                        SymbolKind::Function | SymbolKind::Class | SymbolKind::Constant
                    )
            }
            Language::Rust | Language::Go => {
                s.container.is_none()
                    && matches!(
                        s.kind,
                        SymbolKind::Function
                            | SymbolKind::Struct
                            | SymbolKind::Enum
                            | SymbolKind::Trait
                            | SymbolKind::Interface
                            | SymbolKind::TypeAlias
                            | SymbolKind::Constant
                    )
            }
            // A `locals` entry has the `locals` block as its container and is movable
            // all the same; every other nested name is an argument, which is not.
            Language::Hcl => {
                matches!(
                    s.kind,
                    SymbolKind::Block | SymbolKind::Variable | SymbolKind::Module
                ) && (s.container.is_none() || enclosing_locals_block(index, s).is_some())
            }
            Language::Css | Language::Scss => {
                matches!(s.kind, SymbolKind::Selector | SymbolKind::ElementId)
            }
            Language::Markdown => s.kind == SymbolKind::Heading,
            Language::Zig => {
                s.container.is_none()
                    && matches!(
                        s.kind,
                        SymbolKind::Function
                            | SymbolKind::Struct
                            | SymbolKind::Enum
                            | SymbolKind::TypeAlias
                            | SymbolKind::Constant
                            | SymbolKind::Variable
                    )
            }
            // A variable's value depends on when its assignment ran, so only a
            // function is a definition a move can carry.
            Language::Bash => s.container.is_none() && s.kind == SymbolKind::Function,
            // A nested key's path is its address; only a top-level key keeps the same
            // path in another file.
            Language::Yaml | Language::Helm => s.container.is_none() && s.kind == SymbolKind::Key,
            Language::Html | Language::Xml => false,
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
            crate::vfs::write(&path, content).unwrap();
        }
        let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
        (tmp, Index::build_from_scan(&scanned).unwrap())
    }

    fn apply(plan: &MovePlan, path: &Path) -> String {
        let original = crate::vfs::read_to_string(path).unwrap_or_default();
        match plan.edits.edits_for(path) {
            Some(edits) => apply_to_string(&original, edits).unwrap(),
            None => original,
        }
    }

    #[test]
    fn moves_a_function_between_python_modules() {
        let (tmp, index) = workspace(&[
            (
                "helpers.py",
                "def keep():\n    pass\n\ndef moved():\n    return 1\n",
            ),
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
            (
                "app.py",
                "from lib import shared\n\ndef use():\n    return shared()\n",
            ),
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
        assert!(
            app.contains("from ./dest import shared")
                || app.contains("from .dest import shared")
                || app.contains("dest import shared"),
            "got:\n{app}"
        );
    }

    #[test]
    fn typescript_gets_a_named_import() {
        let (tmp, index) = workspace(&[
            ("a.ts", "export function moved() { return 1; }\n"),
            (
                "b.ts",
                "import { moved } from './a';\nexport const x = moved();\n",
            ),
            ("c.ts", "export const y = 2;\n"),
        ]);
        let id = index.find_symbols("moved", None)[0].id;
        let dest = tmp.path().join("c.ts");
        let plan = to_file(&index, id, &dest).unwrap();

        let b = apply(&plan, &tmp.path().join("b.ts"));
        assert!(b.contains("import { moved } from './c';"), "got:\n{b}");
    }

    #[test]
    fn refuses_languages_with_no_derivable_reachability() {
        // An element id is not a name another document imports, so there is nothing a
        // move could repoint and no reachability it could preserve.
        let (tmp, index) = workspace(&[
            ("a.html", "<div id=\"thing\">x</div>\n"),
            ("b.html", "<p>y</p>\n"),
        ]);
        let found = index.find_symbols("thing", None);
        assert!(!found.is_empty(), "the id must be extracted to be refused");
        let err = to_file(&index, found[0].id, &tmp.path().join("b.html")).unwrap_err();
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
    fn refuses_a_move_that_would_change_language() {
        let (tmp, index) = workspace(&[("a.py", "def f():\n    pass\n"), ("b.ts", "\n")]);
        let id = index.find_symbols("f", None)[0].id;
        let err = to_file(&index, id, &tmp.path().join("b.ts"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("cannot change language"), "got: {err}");
    }

    #[test]
    fn relative_module_paths_are_computed_correctly() {
        assert_eq!(
            relative_module(Path::new("/w/src/a.ts"), Path::new("/w/src/b.ts")).as_deref(),
            Some("./b")
        );
        let nested = relative_module(Path::new("/w/src/deep/a.ts"), Path::new("/w/src/b.ts"));
        assert!(
            nested.as_deref().is_some_and(|m| m.contains("b")),
            "got {nested:?}"
        );
    }

    #[test]
    fn python_relative_modules_are_spelled_with_dots() {
        assert_eq!(
            python_relative_module(Path::new("/w/a.py"), Path::new("/w/b.py")).as_deref(),
            Some(".b")
        );
        assert_eq!(
            python_relative_module(Path::new("/w/a.py"), Path::new("/w/sub/b.py")).as_deref(),
            Some(".sub.b")
        );
        assert_eq!(
            python_relative_module(Path::new("/w/sub/a.py"), Path::new("/w/b.py")).as_deref(),
            Some("..b")
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

    #[test]
    fn rust_use_insertion_lands_after_the_header_and_uses() {
        let source = "//! Docs.\nuse std::fmt;\n\nfn f() {}\n";
        let at = rust_use_insertion_point(source);
        assert_eq!(&source[..at], "//! Docs.\nuse std::fmt;\n");
        assert_eq!(rust_use_insertion_point("fn f() {}\n"), 0);
    }

    #[test]
    fn module_declarations_are_recognised_only_in_the_file_form() {
        assert!(declares_module("mod helpers;\n", "helpers"));
        assert!(declares_module("pub mod helpers;\n", "helpers"));
        assert!(declares_module("pub(crate) mod helpers;\n", "helpers"));
        assert!(!declares_module("mod helpers { }\n", "helpers"));
        assert!(!declares_module("mod other;\n", "helpers"));
    }

    #[test]
    fn slugs_follow_the_anchor_spelling() {
        assert_eq!(slug("Title One"), "title-one");
        assert_eq!(slug("  Getting Started!  "), "getting-started");
        assert_eq!(slug("C++ & Rust"), "c--rust");
    }

    #[test]
    fn relative_links_walk_up_and_down() {
        assert_eq!(
            relative_link(Path::new("/w/docs"), Path::new("/w/docs/a.md")).as_deref(),
            Some("a.md")
        );
        assert_eq!(
            relative_link(Path::new("/w/docs/deep"), Path::new("/w/docs/a.md")).as_deref(),
            Some("../a.md")
        );
    }

    #[test]
    fn whole_word_matching_ignores_substrings() {
        assert!(mentions_word("use fmt::Debug;", "fmt"));
        assert!(!mentions_word("format!(\"x\")", "fmt"));
    }
}
