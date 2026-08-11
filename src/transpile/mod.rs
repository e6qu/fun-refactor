//! Rewriting a file as a different programming language.
//!
//! Source → [`ir`] → source. One reader and one writer per language instead of a
//! translator per pair: six languages is thirty ordered pairs and twelve files.
//!
//! # Scope
//!
//! The signature is the contract: every parameter in order, with its type and the
//! return type, carried across with only the spelling changed to the target's
//! convention. A type with no counterpart goes through by name and is counted, so a
//! substitution never passes silently.
//!
//! Also translated: declarations, records, and the parts of a body that mean the same
//! thing in every language — a return, a binding, a branch, a loop over a collection, a
//! call. A record becomes a Rust `struct` with an `impl`, a Python `@dataclass`, a Go
//! `struct` with methods beside it, or a TypeScript `interface` or `class` depending on
//! whether it has behaviour.
//!
//! # Limits
//!
//! The result is a draft. Ownership, goroutines, decorators, generators,
//! comprehensions, pattern matching and error propagation have no general translation,
//! so each goes into the output verbatim, inside a comment, under a marker.
//!
//! Every translation returns a [`Fidelity`] recording how much of it is real.

pub mod ir;
pub mod nextjs;
mod read;
mod write;

/// What a file says, in the form no one language owns.
///
/// The first half of [`plan`], on its own. A caller that wants to *compare* two files
/// written in different languages has nowhere else to stand: the only thing they have
/// in common is what this returns.
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
             be a guess about broken code",
            path.display()
        );
    }
    read::read(language, &source, parsed.root())
}

/// Read a parsed file into the IR. Used by the framework-aware translations too.
pub(crate) fn read_module(
    language: Language,
    source: &str,
    root: tree_sitter::Node<'_>,
) -> Result<ir::Module> {
    read::read(language, source, root)
}

/// Write a module out as a language. Used by the framework-aware translations too.
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
use std::path::{Path, PathBuf};

/// The languages a file can be translated into.
pub const SUPPORTED: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Java,
    Language::Python,
    Language::TypeScript,
    Language::Zig,
];

/// Whether a file written in this language can be read into the shared representation.
///
/// TSX is TypeScript with JSX in it, and the reader treats it as TypeScript, so a `.tsx`
/// file is a source like any other.
pub fn can_be_read(language: Language) -> bool {
    SUPPORTED.contains(&language) || language == Language::Tsx
}

/// Whether a translation can be written in this language.
///
/// Not TSX. A translation produces no JSX, so writing one into a `.tsx` file names a
/// flavour the content does not have — `typescript` is the target, and `fr translate`
/// turns a `.ts` file into a `.tsx` one where that is what is wanted. Reading and
/// writing were one function, and the list the listing walks was the writing one, so
/// asking for `tsx` worked while nothing ever offered it.
pub fn can_be_written(language: Language) -> bool {
    SUPPORTED.contains(&language)
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
            "there is no writer for {to}. {} cannot be expressed in it without \
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
    let destination = crate::translate::destination_for(path, to)?;
    if crate::vfs::exists(&destination) {
        bail!(
            "{} already exists; translating {} would overwrite it",
            destination.display(),
            path.display()
        );
    }

    let parsers = Parsers::new();
    let parsed = parsers.parse(from, &source)?;
    if parsed.has_errors() {
        bail!(
            "{} does not parse cleanly as {from}, so anything read out of it would be a \
             guess about broken code",
            path.display()
        );
    }

    let mut module = read::read(from, &source, parsed.root())?;
    // Java has no top level below the type, so its writer needs a class to put the
    // module in — and a public class must be named after its file.
    module.name = destination
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string());
    let (mut output, fidelity) = write::write(to, &module)?;

    // A header, because a translated file that does not announce itself will be read
    // as though a person wrote it.
    let header = banner(to, &from.to_string(), path, &fidelity);
    output.insert_str(0, &header);

    // The output must be a file the target's own grammar accepts. The edit engine
    // would catch it too, but only as "would not parse after the change", which says
    // nothing about which construct did it. Checked here, where the answer is known.
    let written = parsers.parse(to, &output)?;
    if written.has_errors() {
        let at = first_error(&written, &output)
            .map(|at| format!(" — first at line {}, column {}", at.line, at.col))
            .unwrap_or_default();
        bail!(
            "the {to} this produced does not parse{at}. That is a defect in the \
             translator and not in your file; the output is not written.\n\n{}",
            numbered(&output, first_error(&written, &output).map(|at| at.line))
        );
    }

    let mut edits = EditSet::new();
    edits.add(
        destination.clone(),
        Edit::new(
            crate::span::Span::new(0, 0),
            &output,
            format!("translate {} to {to}", path.display()),
        ),
    );

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

/// Line and column of the most specific syntax error in a parse.
///
/// **Deepest, not first.** An error node can swallow the whole file — when the very
/// first construct is wrong, tree-sitter's outermost `ERROR` starts at byte zero — and
/// reporting that says "line 1, column 1" and prints the banner, which tells whoever
/// reads the defect report nothing at all. The innermost error is where the parser
/// actually gave up, and a `MISSING` node beats an `ERROR` at the same depth because
/// it names what was expected and not merely where.
fn first_error(parsed: &crate::parse::Parsed, source: &str) -> Option<crate::span::LineCol> {
    let mut cursor = parsed.root().walk();
    let mut stack = vec![(parsed.root(), 0usize)];
    // Depth, then "missing beats error", then earliest — in that order.
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
    // A parse can report an error that this walk does not find: an empty Zig struct
    // holds a zero-width missing identifier that `Node::children` does not yield, and
    // the message came out with no position at all. `error_spans` walks with a cursor
    // and does find it, so it is the fallback and not a second opinion.
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
fn banner(to: Language, from: &str, source: &Path, fidelity: &Fidelity) -> String {
    let comment = |line: &str| match to {
        Language::Python => format!("# {line}\n"),
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
    if fidelity.translated() == 0 {
        out.push_str(&comment(
            "Nothing was found to translate: no function, record or constant. If the \
             source has any, this tool did not recognise them.",
        ));
    } else if fidelity.is_complete() {
        out.push_str(&comment(
            "Every signature carried across with its types intact.",
        ));
    } else {
        if fidelity.signatures_with_foreign_types > 0 {
            out.push_str(&comment(&format!(
                "{} signature(s) mention a type this tool does not know; they were \
                 written through by name and are not checked.",
                fidelity.signatures_with_foreign_types
            )));
        }
        if fidelity.imports_listed > 0 {
            out.push_str(&comment(&format!(
                "{} import(s) are listed as comments: dependencies do not carry between \
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
    out.push('\n');
    out
}
