//! Rewriting a file as a different programming language.

pub mod fastapi;
pub mod ir;
pub mod nextjs;
mod normalize;
mod read;
pub mod routes;
pub mod scaffold;
pub mod tfjson;
mod write;

/// What a file says, in the form no one language owns.
pub fn read_file(path: &Path) -> Result<ir::Module> {
    let Some(language) = crate::lang::detect(path) else {
        bail!("{} is not a language this build recognises", path.display());
    };
    if !can_be_read(language) {
        bail!("there is no reader for {language}");
    }
    let source = crate::vfs::read_to_string(path)?;
    let parsed = Parsers::new().parse(language, &source)?;
    if parsed.has_errors() {
        bail!(
            "{} does not parse cleanly as {language}, so anything read out of it would \
             be a guess about broken code.",
            path.display()
        );
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string());
    read::read(language, &source, parsed.root(), stem.as_deref())
}

/// Read a parsed file into the IR.
pub(crate) fn read_module(
    language: Language,
    source: &str,
    root: tree_sitter::Node<'_>,
) -> Result<ir::Module> {
    read::read(language, source, root, None)
}

/// Write a module out as a language.
pub(crate) fn write_module(language: Language, module: &ir::Module) -> Result<(String, Fidelity)> {
    write::write(language, module)
}

/// Write a piece of a file, spelling names the way the whole file does.
pub(crate) fn write_module_in(
    language: Language,
    module: &ir::Module,
    context: &ir::Module,
) -> Result<(String, Fidelity)> {
    write::write_in_context(language, module, context)
}

pub use ir::{Fidelity, Module};
use write::snake_always;
pub use write::MARKER;

use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::parse::Parsers;
use anyhow::{bail, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The languages whose pairwise translation is complete: nothing carried on the
/// corpora, and execution equality across every conformance group.
pub const COMPLETE: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Java,
    Language::Python,
    Language::TypeScript,
    Language::Zig,
];

/// The languages this reads a file out of.
pub const READABLE: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Java,
    Language::Python,
    Language::TypeScript,
    Language::Zig,
    Language::Bash,
];

/// The languages a file may become. Wider than [`READABLE`]: reading a language means
/// deciding what each of its constructs meant, and writing one means only spelling
/// constructs already decided.
pub const WRITABLE: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Java,
    Language::Python,
    Language::TypeScript,
    Language::Zig,
    Language::Bash,
    Language::Lean,
];

/// Whether a reader takes this language into the shared representation.
pub fn can_be_read(language: Language) -> bool {
    READABLE.contains(&language) || language == Language::Tsx
}

/// Whether a writer answers in this language.
pub fn can_be_written(language: Language) -> bool {
    WRITABLE.contains(&language)
}

/// A translation that has been worked out but not applied.
#[derive(Debug)]
pub struct TranslationPlan {
    pub from: Language,
    pub to: Language,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub edits: EditSet,
    pub fidelity: Fidelity,
    /// The translated text, so a caller can show it without applying anything.
    pub output: String,
}

/// Translate `path` into `to`, writing beside it under the target's extension.
pub fn plan(path: &Path, to: Language) -> Result<TranslationPlan> {
    plan_to(path, to, None, false)
}

/// [`plan`], with the destination and the overwrite decision in the caller's hands.
pub fn plan_to(
    path: &Path,
    to: Language,
    out: Option<&Path>,
    force: bool,
) -> Result<TranslationPlan> {
    plan_impl(path, to, out, force, None)
}

/// [`plan_to`], with the rest of a directory sweep in hand.
pub fn plan_to_in_context(
    path: &Path,
    to: Language,
    out: Option<&Path>,
    force: bool,
    context: &ir::Module,
    siblings: &BTreeMap<PathBuf, ir::Module>,
) -> Result<TranslationPlan> {
    plan_impl(path, to, out, force, Some((context, siblings)))
}

type Sweep<'a> = (&'a ir::Module, &'a BTreeMap<PathBuf, ir::Module>);

/// Rename this file's top-level declarations that a sibling declares too.
fn rename_colliding_declarations(
    module: &mut ir::Module,
    path: &Path,
    siblings: &BTreeMap<PathBuf, ir::Module>,
) {
    let declared = |m: &ir::Module| -> BTreeMap<String, ()> {
        m.items
            .iter()
            .filter_map(|item| match item {
                ir::Item::Record(r) => Some((r.name.clone(), ())),
                ir::Item::Sum(s) => Some((s.name.clone(), ())),
                ir::Item::Newtype(n) => Some((n.name.clone(), ())),
                _ => None,
            })
            .collect()
    };
    let mine = declared(module);
    let mut taken: Vec<String> = Vec::new();
    for (other_path, other) in siblings {
        if other_path == path || other_path.as_path() >= path {
            continue;
        }
        for name in declared(other).keys() {
            if mine.contains_key(name) {
                taken.push(name.clone());
            }
        }
    }
    if taken.is_empty() {
        return;
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let prefix = crate::transpile::write::pascal(&stem);
    let mut notes = Vec::new();
    for item in &mut module.items {
        let name = match item {
            ir::Item::Record(r) => &mut r.name,
            ir::Item::Sum(s) => &mut s.name,
            ir::Item::Newtype(n) => &mut n.name,
            _ => continue,
        };
        if taken.contains(name) {
            let renamed = format!("{prefix}{name}");
            notes.push(format!(
                "another file of this sweep declares `{name}`, and every file shares \
                 one namespace here, so this one becomes `{renamed}`."
            ));
            *name = renamed;
        }
    }
    module.sweep_notes.extend(notes);
}

fn plan_impl(
    path: &Path,
    to: Language,
    out: Option<&Path>,
    force: bool,
    sweep: Option<Sweep<'_>>,
) -> Result<TranslationPlan> {
    crate::capabilities::record(crate::capabilities::Capability::Translate, to);
    let Some(from) = crate::lang::detect(path) else {
        bail!("{} is not a language this build recognises", path.display());
    };
    if from == to {
        bail!("{} is already {to}", path.display());
    }
    if !can_be_read(from) {
        bail!(
            "there is no reader for {from}. Translating out of it would mean deciding \
             what its constructs mean in a language that has none of them, and this \
             tool does not guess."
        );
    }
    if !can_be_written(to) {
        bail!(
            "there is no writer for {to}. nothing spells {} in it without \
             inventing structure the source never had.{}",
            from,
            match to {
                Language::Tsx =>
                    " TSX is TypeScript with JSX in it, and a translation \
                                   writes none, so `typescript` is the target here.",
                _ => "",
            }
        );
    }

    let source = crate::vfs::read_to_string(path)?;
    let destination = match out {
        Some(out) => out.to_path_buf(),
        None => crate::translate::destination_for(path, to)?,
    };
    if crate::vfs::exists(&destination) && !force {
        bail!(
            "{} already exists; translating {} would overwrite it. --force \
             overwrites, --out chooses another path.",
            destination.display(),
            path.display()
        );
    }

    let parsers = Parsers::new();
    let parsed = parsers.parse(from, &source)?;
    if parsed.has_errors() {
        bail!(
            "{} does not parse cleanly as {from}, so anything read out of it would be a \
             guess about broken code.",
            path.display()
        );
    }

    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string());
    let mut module = read::read(from, &source, parsed.root(), stem.as_deref())?;
    if let Some((context, siblings)) = sweep {
        // The sweep travels as one unit, so a sibling's declarations are as good as this file's
        // own.
        lift_local_imports(&mut module, from);
        resolve_sibling_imports(&mut module, path, from, siblings);
        if from == Language::Python {
            let types = context
                .items
                .iter()
                .filter_map(|item| match item {
                    ir::Item::Record(r) => Some(r.name.clone()),
                    _ => None,
                })
                .collect();
            read::promote_constructions(&mut module, &types);
        }
        // Two files of a sweep may each declare a `Thing`.
        if to.packages_by_directory() {
            rename_colliding_declarations(&mut module, path, siblings);
        }
    }
    // Java has no top level below the type, so its writer needs a class to hold the module.
    module.name = destination
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string());
    let (mut output, fidelity) = match sweep {
        Some((context, _)) => write::write_in_context(to, &module, context)?,
        None => write::write(to, &module)?,
    };

    // A header: a translated file that announces nothing reads
    // as though a person wrote it.
    let header = banner(to, &from.to_string(), path, &fidelity, &module.sweep_notes);
    output.insert_str(0, &header);

    // The output must be a file the target's own grammar accepts.
    let written = parsers.parse(to, &output)?;
    if written.has_errors() {
        let at = first_error(&written, &output)
            .map(|at| format!(". First at line {}, column {}", at.line, at.col))
            .unwrap_or_default();
        bail!(
            "the {to} this produced does not parse{at}. That is a defect in the \
             translator and not in your file; this writes no output.\n\n{}",
            numbered(&output, first_error(&written, &output).map(|at| at.line))
        );
    }

    // Overwriting means replacing.
    let existing = crate::vfs::read_to_string(&destination)
        .map(|s| s.len())
        .unwrap_or(0);
    let mut edits = EditSet::new();
    edits.add(
        destination.clone(),
        Edit::new(
            crate::span::Span::new(0, existing),
            &output,
            format!("translate {} to {to}", path.display()),
        ),
    );
    edits.declare_language(destination.clone(), to);

    Ok(TranslationPlan {
        from,
        to,
        source: path.to_path_buf(),
        destination,
        edits,
        fidelity,
        output,
    })
}

/// Move an import written inside a function body up to the file's own imports.
fn lift_local_imports(module: &mut ir::Module, from: Language) {
    let mut lifted: Vec<ir::Item> = Vec::new();
    for item in &mut module.items {
        let bodies: Vec<&mut Vec<ir::Stmt>> = match item {
            ir::Item::Function(f) => vec![&mut f.body],
            ir::Item::Record(r) => r.methods.iter_mut().map(|m| &mut m.body).collect(),
            _ => continue,
        };
        for body in bodies {
            body.retain(|stmt| {
                let ir::Stmt::Unsupported(carried) = stmt else {
                    return true;
                };
                let Some(target) = read::parse_import(from, &carried.source) else {
                    return true;
                };
                lifted.push(ir::Item::Import {
                    text: carried.source.clone(),
                    line: carried.line,
                    target: Some(target),
                });
                false
            });
        }
    }
    if lifted.is_empty() {
        return;
    }
    // Ahead of the declarations, where a reader looks for what a file brings in.
    let at = module
        .items
        .iter()
        .position(|item| !matches!(item, ir::Item::Import { .. }))
        .unwrap_or(module.items.len());
    for (offset, item) in lifted.into_iter().enumerate() {
        module.items.insert(at + offset, item);
    }
}

/// Point each parsed import at the sweep sibling it names, where it names one.
fn resolve_sibling_imports(
    module: &mut ir::Module,
    path: &Path,
    from: Language,
    siblings: &BTreeMap<PathBuf, ir::Module>,
) {
    let Some(dir) = path.parent() else {
        return;
    };
    for item in &mut module.items {
        let ir::Item::Import {
            target: Some(target),
            ..
        } = item
        else {
            continue;
        };
        if target.names.is_empty() {
            continue;
        }
        let Some(stem) = sibling_stem(from, &target.module) else {
            continue;
        };
        let sibling = siblings.iter().find(|(candidate, _)| {
            candidate.as_path() != path
                && candidate.parent() == Some(dir)
                && candidate
                    .file_stem()
                    .is_some_and(|s| s.to_string_lossy() == stem.as_str())
        });
        let Some((_, declared)) = sibling else {
            continue;
        };
        if target.names.iter().all(|n| declares(declared, &n.name)) {
            target.resolved = Some(stem);
        }
    }
}

/// The sibling file stem an import path names, where it can name one.
fn sibling_stem(from: Language, module: &str) -> Option<String> {
    match from {
        Language::Python => {
            let rest = module.strip_prefix('.').unwrap_or(module);
            if rest.is_empty() || rest.contains('.') {
                return None;
            }
            Some(rest.to_string())
        }
        Language::TypeScript | Language::Tsx => {
            let rest = module.strip_prefix("./")?;
            let rest = rest
                .strip_suffix(".ts")
                .or_else(|| rest.strip_suffix(".tsx"))
                .unwrap_or(rest);
            if rest.is_empty() || rest.contains('/') || rest.contains('.') {
                return None;
            }
            Some(rest.to_string())
        }
        _ => None,
    }
}

/// Does this module declare `name` where an import from a sibling can see it?
fn declares(module: &ir::Module, name: &str) -> bool {
    module.items.iter().any(|item| match item {
        ir::Item::Function(f) => f.exported && f.name == name,
        ir::Item::Record(r) => r.exported && r.name == name,
        ir::Item::Constant(c) => c.exported && c.name == name,
        ir::Item::Newtype(n) => n.exported && n.name == name,
        ir::Item::Sum(s) => {
            s.exported && (s.name == name || s.variants.iter().any(|v| v.name == name))
        }
        _ => false,
    })
}

/// Line and column of the most specific syntax error in a parse.
fn first_error(parsed: &crate::parse::Parsed, source: &str) -> Option<crate::span::LineCol> {
    let mut cursor = parsed.root().walk();
    let mut stack = vec![(parsed.root(), 0usize)];
    // Depth, then "missing beats error", then earliest, in that order.
    let mut best: Option<(usize, bool, usize)> = None;
    while let Some((node, depth)) = stack.pop() {
        if node.is_error() || node.is_missing() {
            let candidate = (depth, node.is_missing(), node.start_byte());
            let better = match best {
                None => true,
                Some((d, missing, at)) => {
                    (candidate.0, candidate.1) > (d, missing)
                        || ((candidate.0, candidate.1) == (d, missing) && candidate.2 < at)
                }
            };
            if better {
                best = Some(candidate);
            }
        }
        for child in node.children(&mut cursor) {
            stack.push((child, depth + 1));
        }
    }
    // A parse can report an error that this walk does not find.
    let at = match best {
        Some((_, _, at)) => at,
        None => parsed.error_spans().first().map(|span| span.start)?,
    };
    let index = crate::span::LineIndex::new(source);
    Some(index.line_col(at, source))
}

/// A few lines around the failure, so a defect report carries its own evidence.
fn numbered(source: &str, around: Option<usize>) -> String {
    let Some(line) = around else {
        return String::new();
    };
    source
        .lines()
        .enumerate()
        // A wider window before than after: the construct that broke it is usually the
        // one *above* the line the parser gave up on.
        .filter(|(i, _)| *i + 1 >= line.saturating_sub(8) && *i < line + 2)
        .map(|(i, text)| format!("  {:>4} {text}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The note at the top of a translated file.
fn banner(
    to: Language,
    from: &str,
    source: &Path,
    fidelity: &Fidelity,
    sweep_notes: &[String],
) -> String {
    let comment = |line: &str| match to {
        Language::Python | Language::Bash => format!("# {line}\n"),
        Language::Lean => format!("-- {line}\n"),
        _ => format!("// {line}\n"),
    };
    let mut out = String::new();
    out.push_str(&comment(&format!(
        "Translated from {from} ({}) by fun-refactor.",
        source.display()
    )));
    out.push_str(&comment(&format!(
        "{} function(s), {} record(s), {} constant(s).",
        fidelity.functions, fidelity.records, fidelity.constants
    )));
    if fidelity.newtypes > 0 {
        out.push_str(&comment(&format!(
            "{} distinct type(s) carried across as this language spells them.",
            fidelity.newtypes
        )));
    }
    if fidelity.sums > 0 {
        out.push_str(&comment(&format!(
            "{} choice type(s) carried across, variants and payloads intact.",
            fidelity.sums
        )));
    }
    if fidelity.translated() == 0 {
        out.push_str(&comment(
            "This found nothing to translate: no function, record or constant. If the \
             source has any, this tool did not recognise them.",
        ));
    } else if fidelity.is_complete() && fidelity.signatures_untyped == 0 {
        out.push_str(&comment(
            "Every signature carried across with its types intact.",
        ));
    } else {
        if fidelity.signatures_untyped > 0 {
            out.push_str(&comment(&format!(
                "{} signature(s) have a parameter or a return the source never typed; \
                 the widest type this language has stands in.",
                fidelity.signatures_untyped
            )));
        }
        if fidelity.signatures_with_foreign_types > 0 {
            out.push_str(&comment(&format!(
                "{} signature(s) mention a type this tool does not know; each one \
                 crossed by name alone, and nothing checks it.",
                fidelity.signatures_with_foreign_types
            )));
        }
        if fidelity.imports_listed > 0 {
            out.push_str(&comment(&format!(
                "{} import(s) ride as comments: dependencies do not carry between \
                 these languages.",
                fidelity.imports_listed
            )));
        }
        if fidelity.carried_verbatim > 0 {
            out.push_str(&comment(&format!(
                "{} construct(s) had no counterpart and are below as comments, marked \
                 `{MARKER}`. THIS FILE IS A DRAFT.",
                fidelity.carried_verbatim
            )));
        }
    }
    // A sweep can rename a declaration this file owns, which the file itself has no way to
    // know.
    for note in sweep_notes {
        out.push_str(&comment(note));
    }
    out.push('\n');
    out
}

/// Test-only taps under the reparse gate, for inspecting a draft a plan refuses.
#[doc(hidden)]
pub fn debug_read(
    language: Language,
    source: &str,
    root: tree_sitter::Node<'_>,
) -> anyhow::Result<ir::Module> {
    read::read(language, source, root, None)
}

#[doc(hidden)]
pub fn debug_write(language: Language, module: &ir::Module) -> anyhow::Result<String> {
    Ok(write::write(language, module)?.0)
}
