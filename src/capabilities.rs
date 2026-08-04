//! What this tool can do, per language, derived from the code that decides it.
//!
//! The feature × language matrix used to live only in the README, transcribed by
//! hand. It drifted: operations gated by an explicit predicate stayed accurate, while
//! the ones left to emerge from grammar shape quietly did not — inline-call was
//! documented for six languages and worked for two.
//!
//! So the table is computed here by asking each refactoring's own predicate, and a
//! test checks the published matrix against it. A capability cannot be claimed unless
//! the code agrees.

use crate::lang::{Language, LanguageClass};
use serde::Serialize;

/// Something the tool can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    Symbols,
    Rename,
    SafeDelete,
    Impact,
    Restructure,
    CallGraph,
    Flow,
    Provenance,
    EntryPoints,
    ExtractVariable,
    ExtractFunction,
    InlineVariable,
    InlineCall,
    ChangeSignature,
    MicroRewrites,
    OrganizeImports,
    RemoveFlag,
    MoveToFile,
    Stitch,
    Duplicates,
    DeadCode,
    Translate,
    Openapi,
}

impl Capability {
    pub const ALL: &'static [Capability] = &[
        Capability::Symbols,
        Capability::Rename,
        Capability::SafeDelete,
        Capability::Impact,
        Capability::Restructure,
        Capability::CallGraph,
        Capability::Flow,
        Capability::Provenance,
        Capability::EntryPoints,
        Capability::ExtractVariable,
        Capability::ExtractFunction,
        Capability::InlineVariable,
        Capability::InlineCall,
        Capability::ChangeSignature,
        Capability::MicroRewrites,
        Capability::OrganizeImports,
        Capability::RemoveFlag,
        Capability::MoveToFile,
        Capability::Stitch,
        Capability::Duplicates,
        Capability::DeadCode,
        Capability::Translate,
        Capability::Openapi,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::Symbols => "symbols/def/refs",
            Capability::Rename => "rename",
            Capability::SafeDelete => "safe delete",
            Capability::Impact => "impact",
            Capability::Restructure => "restructure",
            Capability::CallGraph => "call graph",
            Capability::Flow => "flow",
            Capability::Provenance => "provenance",
            Capability::EntryPoints => "entry points",
            Capability::ExtractVariable => "extract variable",
            Capability::ExtractFunction => "extract function",
            Capability::InlineVariable => "inline variable",
            Capability::InlineCall => "inline call",
            Capability::ChangeSignature => "change signature",
            Capability::MicroRewrites => "micro-rewrites",
            Capability::OrganizeImports => "organize imports",
            Capability::RemoveFlag => "remove flag",
            Capability::MoveToFile => "move to file",
            Capability::Stitch => "config→code stitch",
            Capability::Duplicates => "duplicate code",
            Capability::DeadCode => "dead code",
            Capability::Translate => "write as another language",
            Capability::Openapi => "declared HTTP contract",
        }
    }

    /// The command that offers it.
    pub fn command(&self) -> &'static str {
        match self {
            Capability::Symbols => "fr symbols / def / refs",
            Capability::Rename => "fr rename",
            Capability::SafeDelete => "fr delete",
            Capability::Impact => "fr impact",
            Capability::Restructure => "fr restructure",
            Capability::CallGraph => "fr callers / callees / graph",
            Capability::Flow => "fr flow",
            Capability::Provenance => "fr flow",
            Capability::EntryPoints => "fr entrypoints",
            Capability::ExtractVariable => "fr extract",
            Capability::ExtractFunction => "fr extract --function",
            Capability::InlineVariable => "fr inline",
            Capability::InlineCall => "fr inline --call",
            Capability::ChangeSignature => "fr signature",
            Capability::MicroRewrites => "fr rewrite",
            Capability::OrganizeImports => "fr imports",
            Capability::RemoveFlag => "fr remove-flag",
            Capability::MoveToFile => "fr move",
            Capability::Stitch => "fr stitch",
            Capability::Duplicates => "fr duplicates",
            Capability::DeadCode => "fr unused",
            Capability::Translate => "fr translate",
            Capability::Openapi => "fr openapi",
        }
    }
}

/// Whether a capability applies to a language, and why not when it does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "support", rename_all = "kebab-case")]
pub enum Support {
    /// Implemented and tested.
    Yes,
    /// Meaningless for this language — there is nothing the operation could do.
    NotApplicable { because: &'static str },
    /// Meaningful in principle, refused in practice, with the blocking reason.
    Refused { because: &'static str },
}

impl Support {
    pub fn is_yes(&self) -> bool {
        matches!(self, Support::Yes)
    }

    /// The symbol used in the published matrix.
    pub fn mark(&self) -> &'static str {
        match self {
            Support::Yes => "✓",
            Support::NotApplicable { .. } => "n/a",
            Support::Refused { .. } => "—",
        }
    }

    pub fn reason(&self) -> Option<&'static str> {
        match self {
            Support::Yes => None,
            Support::NotApplicable { because } | Support::Refused { because } => Some(because),
        }
    }
}

const NO_BINDING_FORM: &str =
    "markup has no binding form: a reusable value here is a CSS custom property, which \
     belongs to the stylesheet rather than the document";
const NO_CALLABLES: &str = "this language has no functions, so there is nothing to call";
const NO_SUBSTITUTION: &str =
    "this language executes rather than substitutes, so dataflow answers this instead";

/// Why a capability is absent, said about *this* language.
///
/// The fallback reasons here were written when every unsupported language was markup
/// or configuration, and they say so. Adding an imperative language made six of them
/// false at once: `extract variable` told a reader that **Java** "has no binding form:
/// a reusable value here is a CSS custom property", and `entry points` told them that
/// Java "is a stylesheet". A reason that is untrue about the language it is given for
/// is worse than no reason at all, because the whole point of this table is that the
/// empty cells explain themselves.
///
/// `structural` is why the operation is *meaningless* — the right answer for a
/// language that genuinely has no such construct. `missing` is what it would take for
/// one that has the construct and is simply not wired up yet.
fn absent(language: Language, structural: &'static str, missing: &'static str) -> Support {
    if language.class() == LanguageClass::Imperative {
        Support::NotApplicable { because: missing }
    } else {
        Support::NotApplicable {
            because: structural,
        }
    }
}

/// Does `capability` apply to `language`?
///
/// Every arm either calls the predicate the refactoring itself uses, or states why the
/// operation is meaningless. Nothing here is a transcription.
pub fn support(capability: Capability, language: Language) -> Support {
    use Capability as C;
    let imperative = language.class() == LanguageClass::Imperative;

    match capability {
        // The resolution layer serves every language.
        C::Symbols | C::Rename | C::SafeDelete | C::Impact => Support::Yes,

        C::Restructure => Support::Yes,

        C::CallGraph | C::Flow => {
            if imperative {
                Support::Yes
            } else {
                absent(
                    language,
                    NO_CALLABLES,
                    "inlining a call here means substituting a body written with types \
                     this tool does not track",
                )
            }
        }

        C::Provenance => {
            if imperative {
                Support::NotApplicable {
                    because: NO_SUBSTITUTION,
                }
            } else {
                Support::Yes
            }
        }

        C::EntryPoints => {
            let catalog =
                crate::analysis::entrypoints::Catalog::builtin().expect("built-in catalogs parse");
            if crate::analysis::entrypoints::has_rules_for(&catalog, language) {
                Support::Yes
            } else {
                absent(
                    language,
                    // Neutral, because this arm catches YAML as well as stylesheets and
                    // the old wording told a reader that a values file was a stylesheet.
                    "nothing here is executed, so there is no point at which a runtime \
                     starts",
                    "no entry-point rules are written for this language yet; they are \
                     catalogue data rather than code, so adding them is a file under \
                     `catalogs/` naming what a runtime here is pointed at",
                )
            }
        }

        C::ExtractVariable => {
            if crate::refactor::extract::supports_extract(language) {
                Support::Yes
            } else {
                absent(
                    language,
                    NO_BINDING_FORM,
                    "a declaration here needs a written type and there is no inference \
                     in this tool, so nearly every selection would have to refuse",
                )
            }
        }

        C::ExtractFunction => {
            if crate::refactor::extract::supports_extract_function(language) {
                Support::Yes
            } else if language == Language::Zig {
                Support::Refused {
                    because: "Zig requires a written type on every parameter and there is \
                              no inference here, so nearly every selection would refuse",
                }
            } else {
                absent(
                    language,
                    "this language has nothing callable to extract into",
                    "a method here needs a written return type and modifiers, and \
                     choosing them is a judgement about the code rather than a fact \
                     about the selection",
                )
            }
        }

        C::InlineVariable => {
            if crate::refactor::extract::supports_extract(language) {
                Support::Yes
            } else {
                absent(
                    language,
                    NO_BINDING_FORM,
                    "a declaration here needs a written type and there is no inference \
                     in this tool, so nearly every selection would have to refuse",
                )
            }
        }

        C::InlineCall => {
            if crate::refactor::inline::supports_call(language) {
                Support::Yes
            } else if language == Language::Bash {
                Support::NotApplicable {
                    because: "a shell function returns a status, not a value, so a call \
                              is a statement rather than an expression to substitute",
                }
            } else {
                absent(
                    language,
                    NO_CALLABLES,
                    "inlining a call here means substituting a body written with types \
                     this tool does not track",
                )
            }
        }

        C::ChangeSignature => {
            // Bash has no declaration to edit, but a shell function's signature is its
            // positional numbering, and that is exactly what changes at call sites.
            if imperative || matches!(language, Language::Hcl | Language::Scss) {
                Support::Yes
            } else {
                absent(
                    language,
                    NO_CALLABLES,
                    "inlining a call here means substituting a body written with types \
                     this tool does not track",
                )
            }
        }

        C::MicroRewrites => {
            if crate::refactor::rewrite::supported(language) {
                Support::Yes
            } else {
                absent(
                    language,
                    "there is no conditional to invert or guard",
                    "the conditional shapes here are not wired into the rewrite engine yet",
                )
            }
        }

        // Asked of the operation rather than restated here; the two had drifted.
        C::OrganizeImports => match crate::refactor::imports::why_not_organizable(language) {
            None => Support::Yes,
            Some(because) => Support::NotApplicable { because },
        },

        C::RemoveFlag => {
            if crate::refactor::cascade::supports_cascade(language) {
                Support::Yes
            } else {
                absent(
                    language,
                    "there is no conditional here for a flag to guard, so removing one is \
                     a rename or a delete rather than a cascade",
                    "the cascade knows how to fold a constant into the conditionals of \
                     some languages and this is not one of them yet",
                )
            }
        }

        // Two routes, and they promise different things. A *containment* rewrite is
        // the same bytes under another grammar — CSS as SCSS — and loses nothing. A
        // *translation* between programming languages is a draft: signatures carry and
        // anything with no counterpart is carried into the output as a comment.
        C::Translate => {
            let containment = !crate::translate::targets(language).is_empty();
            if crate::transpile::supports(language) || containment {
                Support::Yes
            } else if language.class() == LanguageClass::Imperative {
                Support::NotApplicable {
                    because: "there is no reader or writer for this language yet — a \
                              translation needs one of each, and until both exist the \
                              honest answer is that it cannot be written as anything",
                }
            } else {
                Support::NotApplicable {
                    because: "no other grammar in this build contains this one, so there \
                              is nothing it can be rewritten as without changing it",
                }
            }
        }

        // The contract lives in the *tree* — a Next.js route's URL is where its file
        // sits — so this is a question about one framework rather than one language.
        C::Openapi => match language {
            Language::TypeScript | Language::Tsx => Support::Yes,
            _ => Support::NotApplicable {
                because: "this derives an OpenAPI document from a Next.js route tree, \
                          and a route file is TypeScript",
            },
        },

        C::MoveToFile => match crate::refactor::move_symbol::why_not_move(language) {
            // Asked of the operation rather than restated here. The table said Java
            // could be moved and the operation refused it, which is the table lying
            // about the tool in the tool's own words.
            None => Support::Yes,
            Some(because) => Support::NotApplicable { because },
        },

        // Reachability needs somewhere to start and edges to follow, which the
        // imperative languages have. A configuration language has neither, but the
        // question still means something there — a values key or a CSS class nothing
        // references — and is answered by the same reference index.
        C::DeadCode => {
            if crate::refactor::delete::reports_unused(language) {
                Support::Yes
            } else {
                Support::NotApplicable {
                    because: "nothing in this language declares a name that something \
                              else could fail to use",
                }
            }
        }

        // Every language here is parsed into a tree of named nodes, and comparing
        // those is the whole of the analysis. There is nothing to be unable to do.
        C::Duplicates => {
            if crate::analysis::duplicates::supported(language) {
                Support::Yes
            } else {
                Support::NotApplicable {
                    because: "this language is not parsed into comparable structure",
                }
            }
        }

        C::Stitch => match language {
            // A manifest declares the variables.
            Language::Helm | Language::Yaml => Support::Yes,
            // A program reads them.
            Language::Python
            | Language::Go
            | Language::Rust
            | Language::TypeScript
            | Language::Tsx
            | Language::Bash => Support::Yes,
            _ => Support::NotApplicable {
                because: "this language neither declares environment variables nor reads them",
            },
        },
    }
}

/// One row of the matrix.
#[derive(Debug, Serialize)]
pub struct Row {
    pub capability: &'static str,
    pub command: &'static str,
    pub languages: Vec<(&'static str, Support)>,
}

/// The whole matrix.
pub fn matrix() -> Vec<Row> {
    Capability::ALL
        .iter()
        .map(|capability| Row {
            capability: capability.as_str(),
            command: capability.command(),
            languages: Language::ALL
                .iter()
                .map(|language| (language.name(), support(*capability, *language)))
                .collect(),
        })
        .collect()
}

/// Render the matrix as a markdown table.
pub fn render_markdown() -> String {
    let mut out = String::from("| Capability |");
    for language in Language::ALL {
        out.push_str(&format!(" {} |", language.name()));
    }
    out.push_str("\n|---|");
    out.push_str(&"---|".repeat(Language::ALL.len()));
    out.push('\n');

    for row in matrix() {
        out.push_str(&format!("| {} |", row.capability));
        for (_, support) in &row.languages {
            out.push_str(&format!(" {} |", support.mark()));
        }
        out.push('\n');
    }
    out
}

/// Counts for a summary line.
pub fn totals() -> (usize, usize, usize) {
    let mut yes = 0;
    let mut not_applicable = 0;
    let mut refused = 0;
    for row in matrix() {
        for (_, support) in row.languages {
            match support {
                Support::Yes => yes += 1,
                Support::NotApplicable { .. } => not_applicable += 1,
                Support::Refused { .. } => refused += 1,
            }
        }
    }
    (yes, not_applicable, refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cell_has_a_definite_answer() {
        // The point of the table: no capability × language pair may be unstated.
        for capability in Capability::ALL {
            for language in Language::ALL {
                let support = support(*capability, *language);
                if !support.is_yes() {
                    assert!(
                        support.reason().is_some_and(|r| !r.is_empty()),
                        "{} × {language} refuses without saying why",
                        capability.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn the_resolution_layer_serves_every_language() {
        // Rename, delete, impact and reference queries need only the index, which is
        // what makes them the tool's widest reach.
        for language in Language::ALL {
            for capability in [
                Capability::Symbols,
                Capability::Rename,
                Capability::SafeDelete,
                Capability::Impact,
            ] {
                assert!(
                    support(capability, *language).is_yes(),
                    "{} should serve {language}",
                    capability.as_str()
                );
            }
        }
    }

    #[test]
    fn analysis_splits_cleanly_by_language_class() {
        // Every language gets exactly one of dataflow or provenance, never both and
        // never neither.
        for language in Language::ALL {
            let flow = support(Capability::Flow, *language).is_yes();
            let provenance = support(Capability::Provenance, *language).is_yes();
            assert!(
                flow != provenance,
                "{language} has flow={flow} provenance={provenance}"
            );
        }
    }

    #[test]
    fn extract_and_inline_variable_agree() {
        // They are inverses; a language that can do one must do the other.
        for language in Language::ALL {
            assert_eq!(
                support(Capability::ExtractVariable, *language).is_yes(),
                support(Capability::InlineVariable, *language).is_yes(),
                "{language} can extract but not inline, or the reverse"
            );
        }
    }

    #[test]
    fn markdown_renders_a_row_per_capability() {
        let table = render_markdown();
        assert_eq!(
            table.lines().count(),
            Capability::ALL.len() + 2,
            "header, separator, then one row each"
        );
        for capability in Capability::ALL {
            assert!(
                table.contains(capability.as_str()),
                "missing {}",
                capability.as_str()
            );
        }
    }

    #[test]
    fn totals_account_for_every_cell() {
        let (yes, not_applicable, refused) = totals();
        assert_eq!(
            yes + not_applicable + refused,
            Capability::ALL.len() * Language::ALL.len()
        );
        assert!(yes > 0 && not_applicable > 0);
    }
}
