//! What this tool can do, for each language.

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
    DeclaredType,
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
        Capability::DeclaredType,
    ];

    /// The name a person reads in the table, not an identifier: it has spaces and slashes in
    /// it.
    pub fn label(&self) -> &'static str {
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
            Capability::DeclaredType => "declared type",
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
            Capability::DeclaredType => "fr type",
        }
    }
}

/// Whether a capability applies to a language, and why not when it does not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "support", rename_all = "kebab-case")]
pub enum Support {
    /// Implemented and tested.
    Yes,
    /// Meaningless for this language.
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
     belongs to the stylesheet instead of the document";
/// The same absence, for a data format that has no stylesheet to point at.
const NO_NAME_TO_BIND: &str =
    "this format writes a value where the value goes: there is no name to put in \
     front of one and nowhere to put it";
const NO_CALLABLES: &str = "this language has no functions, so there is nothing to call";
const NO_SUBSTITUTION: &str =
    "this language executes and not substitutes, so dataflow answers this instead";

/// Why a capability is absent, for this language.
fn binding_form_reason(language: Language) -> &'static str {
    match language {
        Language::Json | Language::Yaml | Language::Helm => NO_NAME_TO_BIND,
        _ => NO_BINDING_FORM,
    }
}

fn absent(language: Language, structural: &'static str, missing: &'static str) -> Support {
    if language.class() == LanguageClass::Imperative {
        Support::NotApplicable { because: missing }
    } else {
        Support::NotApplicable {
            because: structural,
        }
    }
}

/// Record that a capability ran against a language.
pub fn record(capability: Capability, language: Language) {
    use std::io::Write;
    use std::sync::OnceLock;

    static LOG: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    let path = LOG.get_or_init(|| std::env::var_os("FR_CAPABILITY_LOG").map(Into::into));
    let Some(path) = path else {
        return;
    };
    // Appended and not held: the writer may be one of a dozen processes.
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let line = format!("{}\t{}\n", capability.label(), language.name());
        let _ = file.write_all(line.as_bytes());
    }
}

/// Does this capability take a whole workspace, and not one thing in one language?
pub fn is_whole_workspace(capability: Capability) -> bool {
    use Capability as C;
    matches!(
        capability,
        C::CallGraph | C::EntryPoints | C::Stitch | C::Duplicates | C::DeadCode
    )
}

/// Record a capability that runs over a whole workspace.
pub fn record_workspace(capability: Capability, index: &crate::index::Index) {
    debug_assert!(
        is_whole_workspace(capability),
        "the table holds {} as a whole-workspace capability and it is not one",
        capability.label()
    );
    if std::env::var_os("FR_CAPABILITY_LOG").is_none() {
        return;
    }
    let mut seen: Vec<Language> = index.files().map(|(_, info)| info.language).collect();
    seen.sort_by_key(|l| l.name());
    seen.dedup();
    for language in seen {
        if support(capability, language).is_yes() {
            record(capability, language);
        }
    }
}

/// Does `capability` apply to `language`?
pub fn support(capability: Capability, language: Language) -> Support {
    use Capability as C;
    let imperative = language.class() == LanguageClass::Imperative;

    match capability {
        // The resolution layer serves every language.
        C::Symbols | C::Rename | C::SafeDelete | C::Impact => Support::Yes,

        C::Restructure => Support::Yes,

        // SCSS gets its own reason.
        C::CallGraph if matches!(language, Language::Scss | Language::Sass) => {
            Support::NotApplicable {
                because: "a mixin expands where it stands, so a stylesheet holds no call \
                          for a graph to walk; `fr usages` lists every `@include`",
            }
        }

        C::CallGraph => {
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

        // `fr flow` shared an arm with the call graph.
        C::Flow => {
            if imperative {
                Support::Yes
            } else if crate::analysis::provenance::supports_provenance(language) {
                Support::NotApplicable {
                    because: "this language runs by substitution rather than by execution, so `fr flow` traces its provenance instead; see \
                              that row",
                }
            } else {
                absent(
                    language,
                    "a value lands where the code uses it, so there is no chain to follow",
                    "following a value here means reading a call graph this tool does \
                     not build for it",
                )
            }
        }

        C::Provenance => {
            // Asked of the analysis, which handles five languages and not eight.
            if crate::analysis::provenance::supports_provenance(language) {
                Support::Yes
            } else if imperative {
                Support::NotApplicable {
                    because: NO_SUBSTITUTION,
                }
            } else {
                Support::NotApplicable {
                    because: "this language has no value-substitution model to trace: a value lands where the code uses it",
                }
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
                    "nothing here runs, so there is no point at which a runtime starts",
                    "no entry-point rules cover this language yet; they are catalogue \
                     data and not code, so adding them means a file under \
                     `catalogs/` naming what a runtime here starts from",
                )
            }
        }

        C::ExtractVariable => {
            if crate::refactor::extract::supports_extract(language) {
                Support::Yes
            } else {
                absent(
                    language,
                    binding_form_reason(language),
                    "a declaration here needs a written type and there is no inference \
                     in this tool, so nearly every selection would have to refuse",
                )
            }
        }

        C::ExtractFunction => {
            if crate::refactor::extract::supports_extract_function(language) {
                Support::Yes
            } else if language == Language::Lean {
                // A dependent return type may name the arguments, so there is nothing to
                // read off the selection.
                Support::NotApplicable {
                    because: "a `def` needs a written type, and in a dependently typed \
                              language that type may name the arguments, so choosing one \
                              is a judgement about the code rather than a fact about the \
                              selection",
                }
            } else {
                // Java is the only language left with something callable, so the reason below
                // is Java's.
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
                    binding_form_reason(language),
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
                              is a statement and not an expression to substitute",
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
            // positional numbering, and the call sites change with it.
            if imperative || matches!(language, Language::Hcl | Language::Scss | Language::Sass) {
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
                     a rename or a delete instead of a cascade",
                    "the cascade knows how to fold a constant into the conditionals of \
                     some languages and this is not one of them yet",
                )
            }
        }

        // Two routes with different promises.
        C::Translate => {
            let containment = !crate::translate::targets(language).is_empty();
            if crate::transpile::can_be_read(language) || containment {
                Support::Yes
            } else if crate::transpile::can_be_written(language) {
                Support::NotApplicable {
                    because: "a file becomes this language and nothing reads one back. \
                              The writer exists and the reader does not, so this stands \
                              among the targets `fr translate` offers and never among \
                              the sources it takes",
                }
            } else if language.class() == LanguageClass::Imperative {
                Support::NotApplicable {
                    because: "there is no reader or writer for this language yet. A \
                              translation needs one of each, and until both exist the \
                              honest answer is that no language can hold it",
                }
            } else {
                Support::NotApplicable {
                    because: "no other grammar in this build contains this one, so there \
                              is no other spelling that leaves it as it is",
                }
            }
        }

        // The contract lives in the *tree*.
        C::Openapi => match language {
            Language::TypeScript | Language::Tsx | Language::Python => Support::Yes,
            _ => Support::NotApplicable {
                because: "this reads a route tree, and the ones it knows are a Next.js \
                          tree in TypeScript and a FastAPI router in Python",
            },
        },

        C::MoveToFile => match crate::refactor::move_symbol::why_not_move(language) {
            // Asked of the operation, and not restated here.
            None => Support::Yes,
            Some(because) => Support::NotApplicable { because },
        },

        // Every language here declares names that something else can reference: a function, a
        // values key, a CSS class, a Markdown heading.
        C::DeadCode => Support::Yes,

        // Every language here parses into a tree of named nodes, and comparing those is the
        // whole of the analysis.
        C::Duplicates => Support::Yes,

        C::DeclaredType => {
            // Asked of the analysis and not restated here, so the two cannot drift.
            if crate::analysis::types::supports_declared_type(language) {
                Support::Yes
            } else {
                Support::NotApplicable {
                    because: "this language has nowhere to write a type down, so there \
                              is nothing here for the source to have said",
                }
            }
        }

        C::Stitch => {
            // A manifest declares the variables and a program reads them.
            if matches!(language, Language::Helm | Language::Yaml)
                || crate::analysis::stitch::reads_environment(language)
            {
                Support::Yes
            } else {
                Support::NotApplicable {
                    because: "this language neither declares environment variables nor reads them",
                }
            }
        }
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
            capability: capability.label(),
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
                        capability.label()
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
                    capability.label()
                );
            }
        }
    }

    #[test]
    fn no_language_is_offered_both_dataflow_and_provenance() {
        // The rule that holds is about overlap.
        let mut neither = Vec::new();
        for language in Language::ALL {
            let flow = support(Capability::Flow, *language).is_yes();
            let provenance = support(Capability::Provenance, *language).is_yes();
            assert!(
                !(flow && provenance),
                "{language} claims both dataflow and provenance"
            );
            if !flow && !provenance {
                neither.push(language.name());
            }
        }
        assert_eq!(
            neither,
            ["html", "xml", "markdown"],
            "the languages with no value model of either kind"
        );
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
                table.contains(capability.label()),
                "missing {}",
                capability.label()
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
