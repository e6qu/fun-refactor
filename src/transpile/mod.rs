//! Rewriting a file as a different programming language.
//!
//! Source → [`ir`] → source. One reader and one writer per language rather than a
//! translator per pair: four languages is twelve ordered pairs and eight files.
//!
//! # What it promises
//!
//! **The signature is the contract.** Every parameter, in order, with its type and the
//! return type, carried exactly — only the spelling changes, to the target's
//! convention. Where a type has no counterpart it is written through by name and
//! *counted*, because silently substituting a type is how a signature stops meaning
//! what it said.
//!
//! Declarations, records and the parts of a body that mean the same thing in every
//! language — a return, a binding, a branch, a loop over a collection, a call — are
//! translated. The output is idiomatic: a record becomes a Rust `struct` with an
//! `impl`, a Python `@dataclass`, a Go `struct` with methods beside it, a TypeScript
//! `interface` or `class` depending on whether it has behaviour.
//!
//! # What it does not promise
//!
//! It is not a compiler and the result is a **draft**. Ownership, goroutines,
//! decorators, generators, comprehensions, pattern matching and error propagation have
//! no general translation, and a guess would be worse than a gap. Every one of them is
//! carried into the output verbatim, inside a comment, under a marker — so the result
//! is a file you finish rather than one you have to diff against the original to
//! discover what went missing.
//!
//! Every translation returns a [`Fidelity`] saying exactly how much of it is real.
//! Read it before trusting the file.

pub mod ir;
pub mod nextjs;
mod read;
mod write;

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

/// The languages a file can be translated out of and into.
pub const SUPPORTED: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Java,
    Language::Python,
    Language::TypeScript,
];

pub fn supports(language: Language) -> bool {
    SUPPORTED.contains(&language) || language == Language::Tsx
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
    let Some(from) = crate::lang::detect(path) else {
        bail!("{} is not a language this build recognises", path.display());
    };
    if from == to {
        bail!("{} is already {to}", path.display());
    }
    if !supports(from) {
        bail!(
            "there is no reader for {from}. Translating out of it would mean deciding \
             what its constructs mean in a language that has none of them, and this \
             tool does not guess."
        );
    }
    if !supports(to) {
        bail!(
            "there is no writer for {to}. {} cannot be expressed in it without \
             inventing structure the source never had.",
            from
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
            .map(|(line, column)| format!(" — first at line {line}, column {column}"))
            .unwrap_or_default();
        bail!(
            "the {to} this produced does not parse{at}. That is a defect in the \
             translator rather than in your file; the output is not written.\n\n{}",
            numbered(&output, first_error(&written, &output).map(|(l, _)| l))
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

/// Line and column of the first syntax error in a parse.
fn first_error(parsed: &crate::parse::Parsed, source: &str) -> Option<(usize, usize)> {
    let mut cursor = parsed.root().walk();
    let mut stack = vec![parsed.root()];
    let mut earliest: Option<usize> = None;
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            earliest =
                Some(earliest.map_or(node.start_byte(), |e: usize| e.min(node.start_byte())));
            continue;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    let at = earliest?;
    let index = crate::span::LineIndex::new(source);
    let position = index.line_col(at, source);
    Some((position.line, position.col))
}

/// A few lines around the failure, so a defect report carries its own evidence.
fn numbered(source: &str, around: Option<usize>) -> String {
    let Some(line) = around else {
        return String::new();
    };
    source
        .lines()
        .enumerate()
        .filter(|(i, _)| *i + 1 >= line.saturating_sub(2) && *i < line + 1)
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
