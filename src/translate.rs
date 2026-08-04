//! Rewriting a file as another language.
//!
//! # What this is not
//!
//! It is not a source-to-source translator. This tool parses to syntax trees and
//! splices byte ranges; it has no type system, no semantic model and no notion of
//! runtime behaviour. Rust to Python is a research problem, and a button claiming to
//! do it would be the least honest thing in this codebase. Every pair of imperative
//! languages is refused, by name, with the reason.
//!
//! # What it is
//!
//! Some languages *contain* others. SCSS is a superset of CSS, TSX is TypeScript with
//! JSX, a Helm template is YAML with actions in it, and XHTML is both HTML and XML.
//! Between those, "rewrite this file as that language" is a real and common
//! migration — turning a stylesheet into a Sass entry point, turning a manifest into
//! a chart template — and for a file that uses no feature the target lacks it is a
//! rename plus a proof.
//!
//! # The proof
//!
//! Two things have to hold, and both are checked:
//!
//! 1. The pair must be in [`targets`] — a declared relationship between the two
//!    grammars, not a guess. Without this, an empty file "converts" to anything and a
//!    short shell script might parse as something absurd.
//! 2. The text must parse **cleanly under the target grammar**. The grammar is the
//!    oracle: SCSS that uses nesting will not parse as CSS, and the refusal can say
//!    where. A superset conversion still gets checked, because a claim nobody
//!    verified is how this tool would start being wrong.
//!
//! The result is written beside the original, under the same stem and the target's
//! extension. The original is left alone: a conversion that deletes its input is not
//! reversible by reading the diff, and the diff is the artifact here as everywhere.

use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::parse::Parsers;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// What a file of this language can be rewritten as.
///
/// Only relationships where one grammar contains the other. The direction matters:
/// every CSS file is SCSS, but only some SCSS files are CSS — which is why the parse
/// in [`plan`] is not optional in either direction.
pub fn targets(from: Language) -> &'static [Language] {
    use Language::*;
    match from {
        Css => &[Scss],
        // Downhill needs the parse to agree: nesting, `&`, `$variables`, `@mixin`
        // and `@use` are all SCSS the CSS grammar rejects.
        Scss => &[Css],
        TypeScript => &[Tsx],
        // Only a `.tsx` with no JSX in it is TypeScript.
        Tsx => &[TypeScript],
        // A manifest is a template with no actions in it.
        Yaml => &[Helm],
        Helm => &[Yaml],
        // XHTML is the intersection; the parse decides whether this file is in it.
        Html => &[Xml],
        Xml => &[Html],
        _ => &[],
    }
}

/// Why a language cannot be rewritten as another, in words a person can act on.
///
/// Returned instead of a silent empty list so the interface can say *why* the button
/// is doing nothing, which for the imperative languages is the whole story.
pub fn why_not(from: Language, to: Language) -> String {
    use crate::lang::LanguageClass;
    if from == to {
        return format!("{from} is already {to}");
    }
    if from.class() == LanguageClass::Imperative && to.class() == LanguageClass::Imperative {
        return format!(
            "rewriting {from} as {to} is a translation, not a refactoring: it needs a \
             semantic model of both languages, and this tool has neither. Nothing here \
             can do it, so nothing here pretends to."
        );
    }
    format!(
        "{to} does not contain {from}: there is no rule that turns one into the other \
         without inventing meaning"
    )
}

/// Why a language can be rewritten as nothing at all.
///
/// Said once, rather than by picking an arbitrary target and explaining that pair.
pub fn why_nothing(from: Language) -> String {
    use crate::lang::LanguageClass;
    if from.class() == LanguageClass::Imperative {
        format!(
            "{from} is a programming language. Rewriting one as another is a translation, \
             not a refactoring: it needs a semantic model of both, and this tool has \
             neither — it parses syntax and splices byte ranges. Nothing here can do it, \
             so nothing here pretends to."
        )
    } else {
        format!("no other grammar this build has contains {from}")
    }
}

/// A rewrite that has been worked out but not applied.
#[derive(Debug)]
pub struct TranslatePlan {
    pub from: Language,
    pub to: Language,
    pub source: PathBuf,
    /// Same stem, same directory, the target's canonical extension.
    pub destination: PathBuf,
    pub edits: EditSet,
}

/// Where the rewritten file goes: same directory, same stem, the target's extension.
pub fn destination_for(path: &Path, to: Language) -> Result<PathBuf> {
    let extension = to
        .extensions()
        .first()
        .ok_or_else(|| anyhow::anyhow!("{to} has no file extension to write"))?;
    let Some(stem) = path.file_stem() else {
        bail!("{} has no file name", path.display());
    };
    let mut destination = path.to_path_buf();
    destination.set_file_name(format!("{}.{}", stem.to_string_lossy(), extension));
    Ok(destination)
}

/// Work out how to rewrite `path` as `to`, refusing when it is not the same file.
pub fn plan(path: &Path, to: Language) -> Result<TranslatePlan> {
    let Some(from) = crate::lang::detect(path) else {
        bail!("{} is not a language this build recognises", path.display());
    };
    if !targets(from).contains(&to) {
        bail!("{}", why_not(from, to));
    }

    let source = crate::vfs::read_to_string(path)?;
    let destination = destination_for(path, to)?;
    if crate::vfs::exists(&destination) {
        bail!(
            "{} already exists; rewriting {} would overwrite it",
            destination.display(),
            path.display()
        );
    }

    // The grammar is the oracle. A superset conversion is still checked, because the
    // supersets are only supersets in the parts of them anyone documents.
    let parsers = Parsers::new();
    if !Parsers::supports(to) {
        bail!("this build has no {to} grammar, so it cannot check the result");
    }
    let parsed = parsers.parse(to, &source)?;
    if parsed.has_errors() {
        let where_ = first_error(&parsed, &source)
            .map(|(line, col)| format!(" — first at line {line}, column {col}"))
            .unwrap_or_default();
        bail!(
            "this file uses {from} the {to} grammar does not accept{where_}. Rewriting it \
             would need a compiler, not a rename."
        );
    }

    // The bytes are unchanged; what changes is which grammar reads them. Writing the
    // whole text as one edit lets the engine's own reparse check see the new file in
    // its new language, which is the same proof again from the other side.
    let mut edits = EditSet::new();
    edits.add(
        destination.clone(),
        Edit::new(
            crate::span::Span::new(0, 0),
            &source,
            format!("rewrite {} as {to}", path.display()),
        ),
    );

    Ok(TranslatePlan {
        from,
        to,
        source: path.to_path_buf(),
        destination,
        edits,
    })
}

/// Line and column of the first syntax error, for a refusal that points at something.
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
