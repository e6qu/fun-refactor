//! Organize imports: drop the ones nothing names, sort the rest.
//!
//! *Removal* follows the index's records: an import goes only when no reference outside
//! an import statement names anything it binds. A glob import binds names nobody can
//! enumerate and a side-effect import binds nothing, so this reports both instead of
//! removing them.
//!
//! Name-based liveness is exact for a value or type that must be spelled where it is
//! used, and blind to whatever a language brings into scope invisibly. `hold_back_reason`
//! lists every such form this tool knows, keeps the import, and says why. Removing a live
//! import breaks a build; keeping a dead one leaves a line.
//!
//! *Sorting* never regenerates import syntax. It reorders each statement's original
//! bytes within one contiguous run of import lines. A blank line, a comment or any other
//! statement ends the run, leaving the programmer's grouping intact. An attribute is not
//! one of those: `#[cfg(…)]` above a `use` is part of that import and moves with it.

use super::{Refusal, Warning, WarningKind};
use crate::edit::{full_line_span, Edit, EditSet};
use crate::index::Index;
use crate::lang::Language;
use crate::model::{Import, SymbolKind};
use crate::parse::{Parsed, Parsers};
use crate::span::{LineIndex, Span};
use anyhow::Result;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use tree_sitter::Node;

/// An import reorganisation that has been worked out but not applied.
#[derive(Debug)]
pub struct ImportsPlan {
    pub file: PathBuf,
    pub language: Language,
    pub edits: EditSet,
    pub warnings: Vec<Warning>,
    /// Statements dropped because nothing in the file names what they bind.
    pub removed: Vec<RemovedImport>,
    /// Number of contiguous blocks whose statements changed order.
    pub sorted_blocks: usize,
    /// What each touched statement's lines become, with no reordering in it.
    ///
    /// [`ImportsPlan::edits`] carries the reordering too, which is right for the command
    /// and wrong for a caller that only wants the dead names gone: a flag removal must not
    /// also sort somebody's imports. A dropped statement maps to nothing, and a narrowed
    /// one to the same statement without the names that died.
    pub replacements: Vec<(Span, String)>,
}

/// Uses a language makes of an imported name without ever spelling it where a query
/// can see it.
///
/// Collected once per file from the parse tree, because every entry is a construct the
/// fact queries do not report as a reference. Each set is consulted only when
/// name-based liveness has already failed, so the cost is paid on the file, not on the
/// import.
#[derive(Debug, Default)]
struct InvisibleUses {
    /// Python names re-exported through `__all__`.
    reexported: HashSet<String>,
    /// TypeScript names appearing inside a `{...}` type in a JSDoc comment.
    jsdoc_types: HashSet<String>,
    /// TypeScript names given to a `@jsx` / `@jsxFrag` / `@jsxImportSource` pragma.
    jsx_pragma: HashSet<String>,
    /// TypeScript names used under a `typeof` type query.
    type_queries: HashSet<String>,
    /// Spans of TypeScript import statements carrying a `type` modifier.
    type_only: HashSet<Span>,
}

impl InvisibleUses {
    fn collect(language: Language, parsed: &Parsed, source: &str) -> Self {
        let mut uses = InvisibleUses::default();
        match language {
            Language::Python => for_each_node(parsed.root(), |node| {
                if !matches!(node.kind(), "assignment" | "augmented_assignment") {
                    return;
                }
                let names_all = node
                    .child_by_field_name("left")
                    .is_some_and(|left| text_of(left, source) == "__all__");
                if names_all {
                    for_each_node(node, |inner| {
                        if inner.kind() == "string_content" {
                            uses.reexported.insert(text_of(inner, source).to_string());
                        }
                    });
                }
            }),
            Language::TypeScript | Language::Tsx => {
                for_each_node(parsed.root(), |node| match node.kind() {
                    "comment" => {
                        let text = text_of(node, source);
                        uses.jsdoc_types.extend(braced_identifiers(text));
                        uses.jsx_pragma.extend(jsx_pragma_names(text));
                    }
                    "type_query" => {
                        for_each_node(node, |inner| {
                            if matches!(inner.kind(), "identifier" | "type_identifier") {
                                uses.type_queries.insert(text_of(inner, source).to_string());
                            }
                        });
                    }
                    "import_statement" => {
                        // The `type` modifier of `import type {...}` and of an inline
                        // `{ type X }` is an anonymous token, so a named-node pattern
                        // cannot see it but a full cursor walk can.
                        let mut type_only = false;
                        for_each_node(node, |inner| {
                            if inner.kind() == "type" && !inner.is_named() {
                                type_only = true;
                            }
                        });
                        if type_only {
                            uses.type_only.insert(Span::from(node));
                        }
                    }
                    _ => {}
                });
            }
            _ => {}
        }
        uses
    }
}

/// Why an import that nothing names is kept anyway, or `None` if it can go.
///
/// Every arm answers the same question for one language: is there a way this binding
/// could be in use that no reference records? The Rust arm is the oldest and states the
/// principle — a trait is used through its methods, so its name never appears at the
/// call site — and the rest follow it. A returned reason is reported verbatim as a
/// warning, so it has to say which binding and why.
fn hold_back_reason(
    language: Language,
    statement: &Statement,
    uses: &InvisibleUses,
) -> Option<String> {
    match language {
        // Any upper-camel-case name may be a trait, and there is no way to tell from
        // syntax alone. Treating them all as possible traits costs some unused type
        // imports left in place, and buys never silently breaking a build.
        Language::Rust => {
            let binding = statement
                .bindings
                .iter()
                .find(|binding| binding.chars().next().is_some_and(char::is_uppercase))?;
            Some(format!(
                "'{}' binds '{binding}', which nothing names — but a trait is used \
                 through its methods, never by name, so this is kept. Remove it by \
                 hand if it really is unused",
                statement.path
            ))
        }

        Language::Python => {
            // `from __future__ import annotations` changes how the whole file is
            // compiled. The name it binds is never meant to be mentioned again.
            if statement.path == "__future__" {
                return Some(format!(
                    "'{}' is a __future__ import: it changes how the file is compiled \
                     instead of binding a name anyone spells, so it is never removed",
                    statement.path
                ));
            }
            if let Some(binding) = statement
                .bindings
                .iter()
                .find(|binding| uses.reexported.contains(*binding))
            {
                return Some(format!(
                    "'{}' binds '{binding}', which nothing names but __all__ re-exports, \
                     so importing it is what publishes it",
                    statement.path
                ));
            }
            // `import myapp.handlers` binds only `myapp`; the point of writing the
            // submodule out is to run its registration side effects at import time.
            if !statement.explicit_binding && statement.path.contains('.') {
                return Some(format!(
                    "'{}' imports a submodule: nothing names the '{}' it binds, but the \
                     statement may exist to run the submodule's registration side \
                     effects, so it is kept",
                    statement.path,
                    statement.bindings.join(", ")
                ));
            }
            None
        }

        Language::TypeScript | Language::Tsx => {
            if uses.type_only.contains(&statement.span) {
                return Some(format!(
                    "'{}' is a type-only import; its uses are all in type positions, \
                     which the fact queries do not capture in full, so it is kept",
                    statement.path
                ));
            }
            if let Some(binding) = statement
                .bindings
                .iter()
                .find(|binding| uses.type_queries.contains(*binding))
            {
                return Some(format!(
                    "'{}' binds '{binding}', which is used in a `typeof {binding}` type \
                     query — a type position no reference records, so it is kept",
                    statement.path
                ));
            }
            if let Some(binding) = statement
                .bindings
                .iter()
                .find(|binding| uses.jsdoc_types.contains(*binding))
            {
                return Some(format!(
                    "'{}' binds '{binding}', which is named in a JSDoc type comment; \
                     that is a use no reference records, so it is kept",
                    statement.path
                ));
            }
            if let Some(binding) = statement
                .bindings
                .iter()
                .find(|binding| uses.jsx_pragma.contains(*binding))
            {
                return Some(format!(
                    "'{}' binds '{binding}', which a JSX pragma comment names as the \
                     factory every JSX element compiles to, so it is kept",
                    statement.path
                ));
            }
            None
        }

        // A Go import binds the imported package's *package clause*, which is a fact
        // about the other package's source. When that source is outside the scan the
        // binding can only be guessed from the path, and the guess is wrong for
        // `gopkg.in/yaml.v2` (package `yaml`), `.../v2` version suffixes and any
        // hyphenated path.
        Language::Go if !statement.explicit_binding && !statement.binding_certain => Some(format!(
            "'{}' is a Go import whose local name is its package clause, and that \
                 package is not in the scan; '{}' is only a guess from the path, so the \
                 import is kept",
            statement.path,
            if statement.bindings.is_empty() {
                "<no name could be guessed>".to_string()
            } else {
                statement.bindings.join(", ")
            }
        )),

        // Zig needs no guard: `@import` yields an ordinary container-level `const`, and
        // every use of it spells that const's name. There is no Zig construct that
        // brings an imported name into scope without naming it — `usingnamespace` does,
        // but it binds nothing for this pass to remove.
        _ => None,
    }
}

/// Call `f` on `node` and every descendant, anonymous tokens included.
fn for_each_node(node: Node, mut f: impl FnMut(Node)) {
    let mut cursor = node.walk();
    loop {
        f(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

fn text_of<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte().min(source.len())]
}

/// Identifiers inside `{...}` in a comment — a JSDoc `@type {Foo}` or `@param {Foo} x`.
///
/// The braces are what makes a JSDoc tag a type annotation, so anything spelled inside
/// them is a name the annotation depends on. `{import('./m').Foo}` yields both `m` and
/// `Foo`, which is the conservative reading.
fn braced_identifiers(comment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in comment.chars() {
        match ch {
            '{' => depth += 1,
            '}' => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                depth = depth.saturating_sub(1);
            }
            _ if depth > 0 && (ch.is_alphanumeric() || ch == '_' || ch == '$') => current.push(ch),
            _ => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
        }
    }
    out.retain(|name| !name.chars().next().is_some_and(|c| c.is_ascii_digit()));
    out
}

/// The name a JSX pragma comment gives, e.g. the `h` of `/** @jsx h */`.
///
/// Every JSX element in the file compiles into a call to that factory, so the import
/// binding it names is used by code that does not exist until after compilation.
fn jsx_pragma_names(comment: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = comment;
    while let Some(at) = rest.find("@jsx") {
        rest = &rest[at + 1..];
        let mut words = rest.split_whitespace();
        let tag = words.next().unwrap_or_default();
        if !matches!(tag, "jsx" | "jsxFrag" | "jsxImportSource" | "jsxRuntime") {
            continue;
        }
        if let Some(word) = words.next() {
            let name: String = word
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !name.is_empty() {
                out.push(name);
            }
        }
    }
    out
}

/// Is a Go import path's last segment usable as the package name without seeing the
/// package clause?
///
/// A plain identifier almost always is. A version suffix (`.../v2`), a `gopkg.in`
/// style `name.vN` segment and anything with a hyphen in it are not: the package clause
/// says something else, and only the imported package's own source can say what.
fn go_binding_is_certain(path: &str) -> bool {
    let Some(last) = path.rsplit('/').find(|segment| !segment.is_empty()) else {
        return false;
    };
    let plain = !last.is_empty()
        && last.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !last.starts_with(|c: char| c.is_ascii_digit());
    let versioned = last.starts_with('v') && last[1..].chars().all(|c| c.is_ascii_digit());
    plain && !versioned
}

/// The package clause of an imported Go package, when the scan can see it.
///
/// The directory a Go package lives in is named by the tail of its import path, but the
/// module prefix (`example.com/app`) lives in `go.mod` and not on disk, so the only
/// thing to match on is how many trailing components agree. The longest agreement wins;
/// two equally good directories disagreeing about the package name means the answer is
/// unknown, not whichever was found first.
fn workspace_package_name(index: &Index, import_path: &str) -> Option<String> {
    let wanted: Vec<&str> = import_path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    if wanted.is_empty() {
        return None;
    }

    let mut best = 0usize;
    let mut found: BTreeSet<String> = BTreeSet::new();
    for (file, info) in index.files() {
        if info.language != Language::Go {
            continue;
        }
        let Some(directory) = file.parent() else {
            continue;
        };
        let on_disk: Vec<String> = directory
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let agreed = (0..wanted.len().min(on_disk.len()))
            .take_while(|i| wanted[wanted.len() - 1 - i] == on_disk[on_disk.len() - 1 - i])
            .count();
        if agreed == 0 || agreed < best {
            continue;
        }
        if agreed > best {
            best = agreed;
            found.clear();
        }
        for id in &info.symbols {
            if let Some(symbol) = index.symbol(*id) {
                if symbol.kind == SymbolKind::Module {
                    found.insert(symbol.name.clone());
                }
            }
        }
    }

    (found.len() == 1).then(|| found.into_iter().next().expect("checked"))
}

/// One import statement the plan removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedImport {
    /// The module path as written.
    pub path: String,
    /// The local names the statement bound, none of which anything used.
    pub bindings: Vec<String>,
    pub span: Span,
    pub line: usize,
}

/// Work out how to organize the imports of one file.
///
/// Refuses for languages that have no import statement to organize, and for files with
/// syntax errors: a use hidden inside an unparsed region would make a removal look
/// safe when it is not.
///
/// Liveness is decided by name. That is exact for a value or type that must be spelled
/// where it is used, and blind to anything a language brings into scope invisibly: a
/// Rust trait imported only so its methods resolve, a Python module imported for its
/// registration side effects under a name that is never mentioned again, a TypeScript
/// type used only in a JSDoc comment. Every such form `hold_back_reason` knows about
/// keeps its import and produces a warning saying which binding and why; check the
/// [`ImportsPlan::removed`] list before committing all the same.
pub fn plan(index: &Index, file: &Path) -> Result<ImportsPlan> {
    // The index first. Reading the file before asking answers "no such file" about a path
    // whose real problem is that nothing indexed it, which is a different thing to fix.
    index
        .file(file)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the index", file.display()))?;
    let source = crate::vfs::read_to_string(file)?;
    plan_in(index, file, &source)
}

/// [`plan`] over source already held in memory.
///
/// The cascade rewrites in memory and re-indexes each round, so the text on disk is the
/// text before it started. Asking this question against that text would answer about the
/// wrong file — and the question is the same one, which is why it is asked here and not
/// answered a second time somewhere else.
pub(crate) fn plan_in(index: &Index, file: &Path, source: &str) -> Result<ImportsPlan> {
    if let Some(info) = index.file(file) {
        crate::capabilities::record(
            crate::capabilities::Capability::OrganizeImports,
            info.language,
        );
    }
    let info = index
        .file(file)
        .ok_or_else(|| anyhow::anyhow!("{} is not in the index", file.display()))?;

    if let Some(reason) = why_not_organizable(info.language) {
        return Err(Refusal::Unsupported {
            operation: "organize imports".into(),
            language: info.language,
            because: reason.to_string(),
        }
        .into());
    }

    if let Some(gap) = info.gaps.first() {
        anyhow::bail!(
            "refusing to organize imports in {}: {}, so a use hidden in the part that did \
             not reach the index could make a live import look unused",
            file.display(),
            gap.cause()
        );
    }

    let line_index = LineIndex::new(source);
    let mut warnings = Vec::new();

    let parsed = Parsers::new().parse(info.language, source)?;
    let mut statements = statements(info.imports.iter(), source, info.language, &parsed);
    // A Go import binds the imported package's package clause. When that package is in
    // the scan its real name is a fact and not a guess, so record it as a binding
    // and stop treating the path as the last word on the subject.
    if info.language == Language::Go {
        for statement in &mut statements {
            if statement.explicit_binding {
                continue;
            }
            if let Some(package) = workspace_package_name(index, &statement.path) {
                statement.binding_certain = true;
                if !statement.bindings.contains(&package) {
                    statement.bindings.push(package);
                    statement.bindings.sort();
                }
            }
        }
    }
    if statements.is_empty() {
        return Ok(ImportsPlan {
            file: file.to_path_buf(),
            language: info.language,
            edits: EditSet::new(),
            warnings,
            removed: Vec::new(),
            sorted_blocks: 0,
            replacements: Vec::new(),
        });
    }

    // Every name used in the file, ignoring the import statements themselves — an
    // import naming `HashMap` is not a use of `HashMap`.
    let mut live: HashSet<&str> = HashSet::new();
    for reference_index in &info.references {
        let reference = &index.references[*reference_index];
        if statements.iter().any(|s| s.span.contains(reference.span)) {
            continue;
        }
        live.insert(reference.name.as_str());
    }

    let invisible = InvisibleUses::collect(info.language, &parsed, source);

    let mut removed = Vec::new();
    let mut drop_statement = vec![false; statements.len()];
    for (i, statement) in statements.iter().enumerate() {
        let position = line_index.line_col(statement.span.start, source);
        if statement.is_glob {
            warnings.push(Warning {
                kind: WarningKind::WeaklyResolved,
                file: file.to_path_buf(),
                line: position.line,
                col: position.col,
                detail: format!(
                    "'{}' is a glob import; what it binds cannot be enumerated, so it is \
                     never removed",
                    statement.path
                ),
            });
            continue;
        }
        // A Go `import _ "embed"` is the one form that binds nothing on purpose; every
        // other empty-binding statement either binds nothing (a TypeScript side-effect
        // import) or defeated the guess, which the language guard below sorts out.
        if statement.bindings.is_empty() && statement.binding_certain {
            warnings.push(Warning {
                kind: WarningKind::WeaklyResolved,
                file: file.to_path_buf(),
                line: position.line,
                col: position.col,
                detail: format!(
                    "'{}' binds no name; it is imported for its side effects and is never \
                     removed",
                    statement.path
                ),
            });
            continue;
        }
        if statement
            .bindings
            .iter()
            .any(|binding| live.contains(binding.as_str()))
        {
            continue;
        }
        // Nothing names it — which for some constructs means nothing *can* name it.
        // Removing one of those leaves a file that still parses but no longer builds,
        // which the reparse check cannot catch, so it is kept and reported instead.
        if let Some(detail) = hold_back_reason(info.language, statement, &invisible) {
            warnings.push(Warning {
                kind: WarningKind::WeaklyResolved,
                file: file.to_path_buf(),
                line: position.line,
                col: position.col,
                detail,
            });
            continue;
        }
        drop_statement[i] = true;
        removed.push(RemovedImport {
            path: statement.path.clone(),
            bindings: statement.bindings.clone(),
            span: statement.span,
            line: position.line,
        });
    }

    // A statement may lose some of what it binds and keep the rest. Dropping only whole
    // statements left `import { up, down }` intact with nothing naming `down`, which is
    // an error under `noUnusedLocals` and a lint failure everywhere else — from the one
    // command whose whole job is removing imports nothing uses.
    let mut narrowed_statements: Vec<(usize, String)> = Vec::new();
    for (i, statement) in statements.iter().enumerate() {
        if drop_statement[i] || statement.is_glob || statement.named.len() < 2 {
            continue;
        }
        let dead: Vec<&NamedImport> = statement
            .named
            .iter()
            .filter(|name| !live.contains(name.local.as_str()))
            // A name that would be held back on its own is held back here too: the
            // question is the same one, asked of one binding instead of all of them.
            .filter(|name| {
                let alone = Statement {
                    bindings: vec![name.local.clone()],
                    named: vec![(*name).clone()],
                    ..(*statement).clone()
                };
                hold_back_reason(info.language, &alone, &invisible).is_none()
            })
            .collect();
        if dead.is_empty() || dead.len() == statement.named.len() {
            continue;
        }
        if let Some(text) = without_names(source, &parsed, statement, &dead) {
            narrowed_statements.push((i, text));
            let position = line_index.line_col(statement.span.start, source);
            removed.push(RemovedImport {
                path: statement.path.clone(),
                bindings: dead.iter().map(|n| n.local.clone()).collect(),
                span: statement.span,
                line: position.line,
            });
        }
    }
    for (i, text) in narrowed_statements {
        statements[i].narrowed = Some(text);
    }

    let mut replacements: Vec<(Span, String)> = Vec::new();
    for (i, statement) in statements.iter().enumerate() {
        if drop_statement[i] {
            replacements.push((statement.lines, String::new()));
        } else if let Some(text) = &statement.narrowed {
            replacements.push((statement.lines, text.clone()));
        }
    }

    let mut edits = EditSet::new();
    let mut sorted_blocks = 0;

    for block in blocks(&statements) {
        let members = &statements[block.clone()];
        if members.iter().any(|s| !s.line_exclusive) {
            let position = line_index.line_col(members[0].span.start, source);
            warnings.push(Warning {
                kind: WarningKind::WeaklyResolved,
                file: file.to_path_buf(),
                line: position.line,
                col: position.col,
                detail: "an import here shares its line with other code; the block was left \
                         untouched and not risk moving that code"
                    .into(),
            });
            continue;
        }

        let region = Span::new(members[0].lines.start, members[members.len() - 1].lines.end);
        let kept: Vec<&Statement> = members
            .iter()
            .enumerate()
            .filter(|(i, _)| !drop_statement[block.start + i])
            .map(|(_, s)| s)
            .collect();

        let before = region.text(source);
        let after = rebuild(&kept, source, before.ends_with('\n'));
        if after == before {
            continue;
        }

        let reordered = kept
            .iter()
            .zip(sorted(&kept))
            .any(|(original, sorted)| original.span != sorted.span);
        if reordered {
            sorted_blocks += 1;
        }

        let dropped = members.len() - kept.len();
        let reason = match (dropped, reordered) {
            (0, _) => "sort import block".to_string(),
            (n, false) => format!("remove {n} unused import(s)"),
            (n, true) => format!("remove {n} unused import(s) and sort the block"),
        };
        edits.add(file.to_path_buf(), Edit::new(region, after, reason));
    }

    warnings.sort_by(|a, b| {
        (a.kind.as_str(), &a.file, a.line, a.col).cmp(&(b.kind.as_str(), &b.file, b.line, b.col))
    });
    warnings.dedup();

    Ok(ImportsPlan {
        file: file.to_path_buf(),
        language: info.language,
        edits,
        warnings,
        removed,
        sorted_blocks,
        replacements,
    })
}

/// Does this language have import statements worth organizing?
///
/// CSS and SCSS are excluded on purpose even though they have `@import`: order there
/// is semantic — a later rule beats an earlier one and `@import` must precede all other
/// rules — so sorting would change what the stylesheet means. The markup and config
/// languages have no import construct at all, and Bash `source` is an executed
/// statement instead of a declaration.
/// Why imports cannot be organized in this language, if they cannot.
///
/// The single authority. The capability table and this operation each kept their own
/// reason, and they drifted: the table told a reader that Bash "has no import
/// statements to organize" while `queries/bash/facts.scm` extracts every `source`.
pub fn why_not_organizable(language: Language) -> Option<&'static str> {
    if organizable(language) {
        return None;
    }
    Some(match language {
        Language::Css | Language::Scss => {
            "CSS @import order is semantic — a later import's rules beat an earlier \
             one's in the cascade, and @import must precede all other rules — so \
             sorting or removing them would change which styles apply"
        }
        // Not a declaration but a command that *runs* the other file: a later `source`
        // may depend on a variable an earlier one set, and a file may be sourced purely
        // for a side effect no name here refers to.
        Language::Bash => {
            "`source` runs the other file instead of declaring a dependency on it, so \
             order carries meaning and a file sourced only for its side effects looks \
             unused"
        }
        _ => "this language has no import statements to organize",
    })
}

pub fn organizable(language: Language) -> bool {
    matches!(
        language,
        Language::Rust
            | Language::Go
            | Language::Python
            | Language::TypeScript
            | Language::Tsx
            | Language::Zig
            | Language::Java
    )
}

/// One import statement, which may correspond to several [`Import`] records: a query
/// reports `use m::{a, b}` once per name, all sharing the statement's span.
#[derive(Debug, Clone)]
struct Statement {
    /// Bytes of the statement itself.
    span: Span,
    /// The whole lines the statement occupies, newline included.
    lines: Span,
    path: String,
    /// Local names the statement introduces.
    bindings: Vec<String>,
    is_glob: bool,
    /// True when nothing but this statement (and whitespace) is on those lines.
    line_exclusive: bool,
    /// True when the statement spells its local name out, so [`Statement::bindings`]
    /// is a reading and not a guess from the path.
    explicit_binding: bool,
    /// True when the bindings are known and not inferred from the import path.
    /// Only Go can be uncertain: the binding is the imported package's package clause.
    binding_certain: bool,
    /// Each name the statement spells out, with the bytes it occupies. A statement that
    /// binds several can lose some and keep the rest, which needs the spans and not only
    /// the names.
    named: Vec<NamedImport>,
    /// Replacement text for [`Statement::lines`], where some of the names it binds went
    /// and the rest stayed.
    narrowed: Option<String>,
}

/// Where a span of the edited text sat before the edits.
///
/// Every edit that ends at or before the span shifts it by the difference between what it
/// removed and what it wrote. A span that overlaps an edit has no position in the original
/// and is answered `None` — an import statement inside a region being rewritten is not one
/// this may also rewrite.
fn before_the_edits(span: Span, edits: &[Edit]) -> Option<Span> {
    let mut shift: isize = 0;
    for edit in edits {
        let removed = edit.span.end.saturating_sub(edit.span.start) as isize;
        let written = edit.replacement.len() as isize;
        let ends_at = (edit.span.start as isize) + shift + written;
        if ends_at <= span.start as isize {
            shift += removed - written;
            continue;
        }
        // Overlapping, or after this span and therefore irrelevant.
        if (edit.span.start as isize) + shift < span.end as isize {
            return None;
        }
    }
    let start = (span.start as isize + shift).try_into().ok()?;
    let end = (span.end as isize + shift).try_into().ok()?;
    Some(Span::new(start, end))
}

/// The edits that drop imports a set of edits would leave with nothing naming them.
///
/// Removing code often removes the last use of an import, and the statement stays behind:
/// `go build` calls that an error and Rust a warning that a `-D warnings` build turns into
/// one. The result parses either way, which is why a sweep for parse errors never sees it.
///
/// Asked by applying the edits to a copy and re-reading, because the question is about the
/// file as it will be and not as it is. Only imports that were live before are touched —
/// one that was already dead is `fr imports`, not this.
pub fn orphaned_by(index: &Index, edits: &EditSet) -> Result<EditSet> {
    let mut out = EditSet::new();
    for (file, file_edits) in edits.iter() {
        let Some(info) = index.file(file) else {
            continue;
        };
        if why_not_organizable(info.language).is_some() {
            continue;
        }
        let Ok(before) = crate::vfs::read_to_string(file) else {
            continue;
        };
        let Ok(after) = crate::edit::apply_to_string(&before, file_edits) else {
            continue;
        };

        let was_dead: Vec<Span> = plan_in(index, file, &before)
            .map(|plan| {
                plan.replacements
                    .into_iter()
                    .map(|(span, _)| span)
                    .collect()
            })
            .unwrap_or_default();

        // The index still describes the file as it was, and the spans below index the
        // text as it will be. Rebuilding it for one file is what keeps the two agreeing.
        let snapshot = vec![(file.clone(), info.language, after.clone())];
        let Ok(reindexed) = Index::build_from_sources(&snapshot) else {
            continue;
        };
        let Ok(plan) = plan_in(&reindexed, file, &after) else {
            continue;
        };
        for (span, replacement) in plan.replacements {
            if was_dead.contains(&span) {
                continue;
            }
            // The span indexes the file as it will be, and the edit set is applied to the
            // file as it is. Every edit that lands before this statement moves it, so the
            // move is undone here rather than two coordinate systems being mixed.
            let Some(original) = before_the_edits(span, file_edits) else {
                continue;
            };
            out.add(
                file.clone(),
                Edit::new(
                    original,
                    replacement,
                    "an import nothing names any more".to_string(),
                ),
            );
        }
    }
    Ok(out)
}

/// One name inside a statement that spells its names out.
#[derive(Debug, Clone)]
struct NamedImport {
    /// The name this binds locally, which is the alias where there is one.
    local: String,
    /// The bytes of the whole clause, `original as local` included.
    span: Span,
}

/// Collapse import records into statements, in source order.
/// Is this the keyword that introduces an import in the language?
fn is_import_keyword(text: &str, language: Language) -> bool {
    let keywords: &[&str] = match language {
        Language::Rust => &["use", "pub use"],
        Language::Go => &["import"],
        Language::Python => &["import", "from"],
        Language::TypeScript | Language::Tsx => &["import", "export"],
        Language::Zig => &["const"],
        _ => &[],
    };
    keywords.contains(&text)
}

fn statements<'a>(
    imports: impl Iterator<Item = &'a Import>,
    source: &str,
    language: Language,
    parsed: &Parsed,
) -> Vec<Statement> {
    let mut grouped: BTreeMap<Span, Vec<&Import>> = BTreeMap::new();
    for import in imports {
        grouped.entry(import.span).or_default().push(import);
    }

    let mut attributes: Vec<Span> = Vec::new();
    for_each_node(parsed.root(), |node| {
        if node.kind().contains("attribute") {
            attributes.push(Span::from(node));
        }
    });
    attributes.sort_by_key(|s| s.end);

    grouped
        .into_iter()
        .filter(|(span, _)| !span.is_empty() && span.end <= source.len())
        .map(|(span, records)| {
            let first = full_line_span(source, span.start);
            let last = full_line_span(source, span.end - 1);
            let lines = Span::new(first.start, last.end.max(first.end).max(span.end));
            let lines = with_attributes(source, lines, &attributes);
            // A statement owns its line if nothing but its own introducing keyword
            // sits before it. Go records `import "os"` as the spec alone, so without
            // this the keyword looks like unrelated code and ends the block.
            // From the statement's own line, not from the attributes above it: an
            // attribute is part of this import, so it does not make the line shared.
            let before = source[first.start..span.start].trim();
            let line_exclusive = (before.is_empty() || is_import_keyword(before, language))
                && source[span.end..lines.end].trim().is_empty();

            let mut bindings = Vec::new();
            let mut named = Vec::new();
            for record in &records {
                bindings.extend(record.names.iter().map(|n| n.local.clone()));
                named.extend(record.names.iter().map(|n| NamedImport {
                    local: n.local.clone(),
                    span: n.span,
                }));
                if let Some(alias) = &record.alias {
                    bindings.push(alias.clone());
                }
            }
            let explicit_binding = !bindings.is_empty();
            if bindings.is_empty() {
                bindings.extend(implicit_binding(&records[0].path, language));
            }
            // Go's `import _ "embed"` binds deliberately nothing.
            bindings.retain(|b| b != "_");
            bindings.sort();
            bindings.dedup();

            let path = records[0].path.clone();
            let binding_certain =
                explicit_binding || language != Language::Go || go_binding_is_certain(&path);

            Statement {
                span,
                lines,
                path,
                bindings,
                is_glob: records.iter().any(|r| r.is_glob),
                line_exclusive,
                explicit_binding,
                binding_certain,
                named,
                narrowed: None,
            }
        })
        .collect()
}

/// The name a whole-module import binds without naming it.
///
/// The three languages that have such a form disagree about which segment it is.
/// `use std::fmt;` binds the last one. `import "net/http"` binds the imported package's
/// package clause, which the path can only suggest — hence the version-suffix and
/// `gopkg.in` handling here and the certainty check in [`go_binding_is_certain`].
/// Python's `import a.b` binds `a`, the *first* segment: the statement makes the whole
/// package reachable, and `b` is only spelled through it.
///
/// TypeScript and Zig have no such form: an import with no named binding there is a
/// side-effect import and binds nothing, so guessing a name from the path would invent
/// a binding that does not exist.
pub(crate) fn implicit_binding(path: &str, language: Language) -> Option<String> {
    let segment = match language {
        Language::Rust => path.rsplit("::").find(|segment| !segment.is_empty())?,
        Language::Python => path
            .trim_start_matches('.')
            .split('.')
            .find(|segment| !segment.is_empty())?,
        Language::Go => {
            let mut segments = path.split('/').filter(|segment| !segment.is_empty());
            let mut last = segments.next_back()?;
            // `example.com/mod/v2` is major-version suffixing: the package is `mod`.
            if last.starts_with('v') && last[1..].chars().all(|c| c.is_ascii_digit()) {
                last = segments.next_back()?;
            }
            // `gopkg.in/yaml.v2` spells the version onto the segment itself.
            last.split('.').next()?
        }
        _ => return None,
    };
    (!segment.is_empty()
        && segment.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !segment.starts_with(|c: char| c.is_ascii_digit()))
    .then(|| segment.to_string())
}

/// Split statements into runs of directly consecutive import lines.
///
/// A blank line, a comment or any other statement between two imports leaves a gap in
/// the line coverage, which ends the run.
/// A statement's lines, extended over the attributes written above it.
///
/// An attribute is part of the item, not a neighbour of it: `#[cfg(feature = "cli")]`
/// above a `use` decides whether that import exists. Sorting moves whole lines, so an
/// attribute left where it was lands on whichever import sorts into its place. This
/// crate's own `src/index.rs` came out of `fr imports` with `use anyhow::…` behind the
/// `cfg` and `use crate::scan::…` unconditional, which compiles under neither setting of
/// the feature.
///
/// Read from the tree and not by looking for `#[`, so a multi-line attribute is one
/// span and a `#[` inside a string is not an attribute at all.
fn with_attributes(source: &str, lines: Span, attributes: &[Span]) -> Span {
    let mut start = lines.start;
    loop {
        let attached = attributes.iter().rev().find(|attribute| {
            attribute.end <= start && source[attribute.end..start].trim().is_empty()
        });
        match attached {
            Some(attribute) if attribute.start < start => {
                start = full_line_span(source, attribute.start).start;
            }
            _ => return Span::new(start, lines.end),
        }
    }
}

fn blocks(statements: &[Statement]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut start = 0;
    for i in 1..statements.len() {
        if statements[i].lines.start != statements[i - 1].lines.end {
            out.push(start..i);
            start = i;
        }
    }
    if !statements.is_empty() {
        out.push(start..statements.len());
    }
    out
}

/// The statements of a block in path order, original order breaking ties.
fn sorted<'a>(statements: &[&'a Statement]) -> Vec<&'a Statement> {
    let mut out = statements.to_vec();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Rebuild a block from the original bytes of the statements that survive.
///
/// Each statement contributes its own line text verbatim, so indentation, spacing and
/// the exact spelling of the statement are carried across untouched. Nothing is
/// regenerated from the parsed import.
/// The bytes a name occupies in an import list, alias and all.
///
/// The index records where a name is *bound*, which for `down as lower` is `lower`. Taking
/// only that out leaves `down as` behind, so the span is widened to the clause the grammar
/// wraps it in — `import_specifier` in TypeScript, `aliased_import` in Python,
/// `use_as_clause` in Rust — stopping before it could swallow the statement itself.
fn whole_clause(parsed: &Parsed, name: Span, statement: Span) -> Span {
    let Some(node) = parsed
        .root()
        .descendant_for_byte_range(name.start, name.end)
    else {
        return name;
    };
    let mut widest = name;
    let mut current = Some(node);
    for _ in 0..3 {
        let Some(here) = current else { break };
        let span = Span::from(here);
        let names_one_import = here.kind().contains("specifier")
            || here.kind().contains("aliased")
            || here.kind().contains("as_clause");
        if names_one_import && span.start >= statement.start && span.end <= statement.end {
            widest = span;
        }
        current = here.parent();
    }
    widest
}

/// The statement's text with some of the names it binds taken out.
///
/// Byte surgery on the names themselves, and not a re-spelling of the statement, because
/// every language writes the list differently — `use a::{b, c};`, `from m import b, c`,
/// `import { b, c } from "m"` — and the separator is the only thing that has to be
/// understood. `None` where taking the names out would leave punctuation this does not
/// know how to close up.
fn without_names(
    source: &str,
    parsed: &Parsed,
    statement: &Statement,
    dead: &[&NamedImport],
) -> Option<String> {
    let start = statement.lines.start;
    let mut text = statement.lines.text(source).to_string();

    // Latest first, so an earlier removal cannot move a later span.
    let mut spans: Vec<Span> = dead
        .iter()
        .map(|name| whole_clause(parsed, name.span, statement.span))
        .collect();
    spans.sort_by_key(|span| std::cmp::Reverse(span.start));

    for span in spans {
        let (from, to) = (span.start.checked_sub(start)?, span.end.checked_sub(start)?);
        if to > text.len() || !text.is_char_boundary(from) || !text.is_char_boundary(to) {
            return None;
        }
        // The comma after it, or the one before it where this was the last in the list.
        let mut cut_to = to;
        while text[cut_to..].starts_with(|c: char| c.is_whitespace()) {
            cut_to += text[cut_to..].chars().next()?.len_utf8();
        }
        let mut cut_from = from;
        if text[cut_to..].starts_with(',') {
            cut_to += 1;
            while text[cut_to..].starts_with(' ') {
                cut_to += 1;
            }
        } else {
            while text[..cut_from].ends_with(|c: char| c.is_whitespace()) {
                cut_from -= text[..cut_from].chars().next_back()?.len_utf8();
            }
            if !text[..cut_from].ends_with(',') {
                return None;
            }
            cut_from -= 1;
            cut_to = to;
        }
        text.replace_range(cut_from..cut_to, "");
    }
    Some(text)
}

fn rebuild(kept: &[&Statement], source: &str, trailing_newline: bool) -> String {
    let parts: Vec<String> = sorted(kept)
        .iter()
        .map(|s| {
            let text = s
                .narrowed
                .as_deref()
                .unwrap_or_else(|| s.lines.text(source));
            text.strip_suffix('\n').unwrap_or(text).to_string()
        })
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    let mut out = parts.join("\n");
    if trailing_newline {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_bindings_only_where_the_language_has_them() {
        assert_eq!(
            implicit_binding("std::fmt", Language::Rust),
            Some("fmt".into())
        );
        assert_eq!(
            implicit_binding("net/http", Language::Go),
            Some("http".into())
        );
        // A major-version suffix is not a package name, and `gopkg.in` writes the
        // version onto the segment itself.
        assert_eq!(
            implicit_binding("example.com/mod/v2", Language::Go),
            Some("mod".into())
        );
        assert_eq!(
            implicit_binding("gopkg.in/yaml.v2", Language::Go),
            Some("yaml".into())
        );
        // Python's `import a.b` binds `a`; `b` is only ever reached through it.
        assert_eq!(
            implicit_binding("os.path", Language::Python),
            Some("os".into())
        );
        // A TS side-effect import binds nothing, so no name may be invented for it.
        assert_eq!(implicit_binding("./polyfills", Language::TypeScript), None);
        // A path segment that is not an identifier cannot be a binding.
        assert_eq!(implicit_binding("zed::*", Language::Rust), None);
    }

    #[test]
    fn organizable_languages_match_the_ones_with_import_declarations() {
        for language in [
            Language::Rust,
            Language::Go,
            Language::Python,
            Language::TypeScript,
            Language::Tsx,
            Language::Zig,
        ] {
            assert!(organizable(language), "{language} has imports to organize");
        }
        for language in [
            Language::Css,
            Language::Scss,
            Language::Html,
            Language::Xml,
            Language::Markdown,
            Language::Yaml,
            Language::Helm,
            Language::Hcl,
            Language::Bash,
        ] {
            assert!(!organizable(language), "{language} must refuse");
        }
    }
}
