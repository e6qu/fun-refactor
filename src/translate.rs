//! Rewriting a file as another language.
//!
//! Two different promises share this command. Where one grammar contains the other, the
//! result is the same bytes under the target's extension, checked by the target's parser.
//! Between programming languages, `src/transpile` produces a draft: signatures carried with
//! their types where possible, and every construct without a counterpart marked, never
//! silently dropped.
//!
//! Some languages contain others. SCSS is a superset of CSS, TSX is TypeScript with JSX, a Helm
//! template is YAML with actions, XHTML is both HTML and XML. For a file using no feature the
//! target lacks, converting between those is a rename plus two checks:
//!
//! 1. The pair appears in [`targets`], a declared relationship between the two grammars.
//!    Without it an empty file would "convert" to anything.
//! 2. The text parses cleanly under the target grammar. SCSS using nesting does not parse as
//!    CSS, and the refusal says where.
//!
//! The result goes beside the original, same stem, target's extension. The original stays: a
//! conversion that deletes its input cannot be reversed from the diff.

use crate::edit::{Edit, EditSet};
use crate::lang::Language;
use crate::parse::Parsers;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// What a file of this language can be rewritten as.
///
/// Only relationships where one grammar contains the other. The direction matters: every CSS
/// file is SCSS, but only some SCSS files are CSS. So the parse in [`plan`] is not optional in
/// either direction.
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

/// One thing a file could be rewritten as, and what that would produce.
#[derive(Debug, Clone)]
pub struct Option_ {
    pub target: Language,
    /// Where the result would be written.
    pub destination: PathBuf,
    /// `None` where the result is the same bytes under a different extension, and the
    /// fidelity of the draft where it is a translation.
    pub fidelity: std::option::Option<crate::transpile::Fidelity>,
    /// Why this target cannot be produced right now, when it cannot.
    ///
    /// A blocked target used to vanish from the listing, and a listing that hides
    /// an entry teaches the reader the pair does not exist.
    pub blocked: std::option::Option<String>,
}

/// Everything `path` could be rewritten as, worked out by asking for each one.
///
/// The list and the answer come from the same call, which keeps them from disagreeing.
/// They were two: the listing walked one set of languages and the request checked another, so
/// `fr translate x.py tsx` succeeded while nothing ever offered it.
///
/// A target that would fail is left out. The reason is available from
/// [`crate::transpile::plan`] or [`plan`] for a caller that asks for it by name.
pub fn options_for(path: &Path) -> Vec<Option_> {
    let Some(from) = crate::lang::detect(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();

    for target in targets(from) {
        let Ok(destination) = destination_for(path, *target) else {
            continue;
        };
        if crate::vfs::exists(&destination) {
            out.push(Option_ {
                target: *target,
                destination,
                fidelity: None,
                blocked: Some(BLOCKED_BY_EXISTING.to_string()),
            });
            continue;
        }
        if let Ok(planned) = plan(path, *target) {
            out.push(Option_ {
                target: *target,
                destination: planned.destination,
                fidelity: None,
                blocked: None,
            });
        }
    }

    // Plus every language this can be translated into, which is a different and much
    // weaker promise, a draft instead of the same bytes.
    if crate::transpile::can_be_read(from) {
        for target in crate::transpile::SUPPORTED {
            if *target == from || out.iter().any(|o| o.target == *target) {
                continue;
            }
            let Ok(destination) = destination_for(path, *target) else {
                continue;
            };
            if crate::vfs::exists(&destination) {
                out.push(Option_ {
                    target: *target,
                    destination,
                    fidelity: None,
                    blocked: Some(BLOCKED_BY_EXISTING.to_string()),
                });
                continue;
            }
            if let Ok(planned) = crate::transpile::plan(path, *target) {
                out.push(Option_ {
                    target: *target,
                    destination: planned.destination,
                    fidelity: Some(planned.fidelity),
                    blocked: None,
                });
            }
        }
    }
    out
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
        let has_reader = crate::transpile::can_be_read(from);
        let has_writer = crate::transpile::can_be_written(to);
        let missing = match (has_reader, has_writer) {
            (false, false) => format!("a reader for {from} and a writer for {to}"),
            (false, true) => format!("a reader for {from}"),
            (true, false) => format!("a writer for {to}"),
            (true, true) => {
                return format!("{from} translates into {to} as a draft; ask for it by name.")
            }
        };
        return format!(
            "translating {from} into {to} needs {missing}, which this build lacks. \
             See `src/transpile/` for the pairs it has."
        );
    }
    format!(
        "{to} does not contain {from}: there is no rule that turns one into the other \
         without inventing meaning"
    )
}

/// Why a language can be rewritten as nothing at all.
///
/// Said once, and not by picking an arbitrary target and explaining that pair.
pub fn why_nothing(from: Language) -> String {
    use crate::lang::LanguageClass;
    if from.class() == LanguageClass::Imperative {
        // This used to say "nothing here can do it. So nothing here pretends to", which was
        // true when it was written and became false the day the transpiler landed: Rust, Go,
        // Python and TypeScript translate into one another. A message that denies a capability
        // the tool has misinforms, because the reader believes it and stops looking.
        let supported: Vec<String> = crate::transpile::SUPPORTED
            .iter()
            .map(|language| language.to_string())
            .collect();
        format!(
            "{from} is a programming language, and there is no reader or writer for it \
             here. Translating between programming languages needs one of each, and this \
             build has both for {}. Adding {from} means writing that pair. See \
             `src/transpile/`.",
            supported.join(", ")
        )
    } else {
        format!("no other grammar this build has contains {from}")
    }
}

/// The one reason a listing shows a target it cannot produce.
pub const BLOCKED_BY_EXISTING: &str =
    "the destination already exists. --force overwrites it, --out chooses another path.";

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
    // Java ties the file's name to the public class inside it. So `sensors.py` has to become
    // `Sensors.java`, not `sensors.java`, which will not compile whatever is written in it.
    let stem = match to {
        Language::Java => pascal_case(&stem.to_string_lossy()),
        _ => stem.to_string_lossy().to_string(),
    };
    let mut destination = path.to_path_buf();
    destination.set_file_name(format!("{stem}.{extension}"));
    Ok(destination)
}

/// `sensor_readings` -> `SensorReadings`, for a language that names files after types.
fn pascal_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = true;
    for c in name.chars() {
        if c == '_' || c == '-' || c == '.' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// Work out how to rewrite `path` as `to`, refusing when it is not the same file.
pub fn plan(path: &Path, to: Language) -> Result<TranslatePlan> {
    plan_to(path, to, None, false)
}

/// [`plan`], with the destination and the overwrite decision in the caller's hands.
pub fn plan_to(
    path: &Path,
    to: Language,
    out: std::option::Option<&Path>,
    force: bool,
) -> Result<TranslatePlan> {
    crate::capabilities::record(crate::capabilities::Capability::Translate, to);
    let Some(from) = crate::lang::detect(path) else {
        bail!("{} is not a language this build recognises", path.display());
    };
    if !targets(from).contains(&to) {
        bail!("{}", why_not(from, to));
    }

    let source = crate::vfs::read_to_string(path)?;
    let destination = match out {
        Some(out) => out.to_path_buf(),
        None => destination_for(path, to)?,
    };
    if crate::vfs::exists(&destination) && !force {
        bail!(
            "{} already exists; rewriting {} would overwrite it. --force overwrites, \
             --out chooses another path.",
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
            .map(|at| format!(". First at line {}, column {}", at.line, at.col))
            .unwrap_or_default();
        bail!(
            "this file uses {from} the {to} grammar does not accept{where_}. Rewriting it \
             would need a compiler, not a rename."
        );
    }

    // The bytes are unchanged; what changes is which grammar reads them. Writing the
    // whole text as one edit lets the engine's own reparse check see the new file in
    // its new language, which is the same proof again from the other side.
    // Overwriting is replacing; see the same edit in `transpile::plan_to`.
    let existing = crate::vfs::read_to_string(&destination)
        .map(|s| s.len())
        .unwrap_or(0);
    let mut edits = EditSet::new();
    edits.add(
        destination.clone(),
        Edit::new(
            crate::span::Span::new(0, existing),
            &source,
            format!("rewrite {} as {to}", path.display()),
        ),
    );
    edits.declare_language(destination.clone(), to);

    Ok(TranslatePlan {
        from,
        to,
        source: path.to_path_buf(),
        destination,
        edits,
    })
}

/// Line and column of the first syntax error, for a refusal that points at something.
fn first_error(parsed: &crate::parse::Parsed, source: &str) -> Option<crate::span::LineCol> {
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
    Some(index.line_col(at, source))
}
