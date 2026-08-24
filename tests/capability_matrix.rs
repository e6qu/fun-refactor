//! The published matrix must match what the code does.
//!
//! This exists because the README's matrix drifted once already: capabilities gated by an
//! explicit predicate stayed accurate while ones left to emerge from grammar shape did not.
//! `inline --call` was documented for six languages while working for two. A table nobody
//! checks is a table that lies.

use fun_refactor::capabilities::{self, Capability, Support};
use fun_refactor::lang::Language;

#[test]
fn the_readme_matrix_matches_the_code() {
    let generated = capabilities::render_markdown();
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md is readable");

    for line in generated.lines() {
        assert!(
            readme.contains(line.trim()),
            "README.md is out of date. Regenerate with `fr capabilities --markdown`.\n\
             Missing row:\n  {line}"
        );
    }
}

#[test]
fn every_command_that_has_a_per_language_answer_is_in_the_matrix() {
    // The matrix is the tool's own claim about what it does, per language. Three commands were
    // missing from it, `fr translate` most conspicuously, since its answer differs by language
    // in two different ways. `fr recipe` is the one genuine exception: it composes the rows
    // instead of adding one.
    let commands: Vec<&str> = Capability::ALL.iter().map(|c| c.command()).collect();
    for expected in [
        "fr rename",
        "fr delete",
        "fr extract",
        "fr inline",
        "fr signature",
        "fr move",
        "fr imports",
        "fr rewrite",
        "fr restructure",
        "fr remove-flag",
        "fr translate",
        "fr openapi",
        "fr unused",
        "fr duplicates",
        "fr entrypoints",
        "fr stitch",
        "fr impact",
        "fr type",
    ] {
        assert!(
            commands.iter().any(|c| c.starts_with(expected)),
            "`{expected}` has a different answer per language and is not in the \
             capability matrix. Every such command belongs there, or the table is \
             claiming to be complete while omitting one."
        );
    }
}

#[test]
fn no_reason_describes_a_different_language() {
    // Six reasons once explained Java's absences in terms of stylesheets and markup,
    // because the fallback strings were written when every unsupported language was
    // one of those. A reason that names a language other than its own is the tell.
    let mut examined = 0;
    for capability in Capability::ALL {
        for language in Language::ALL {
            let Some(reason) = capabilities::support(*capability, *language).reason() else {
                continue;
            };
            examined += 1;
            // A word only some languages can be described with, and the ones that can.
            // The first three caught Java; the rest were added when `extract function`
            // told a reader that a **shell function** needs "a written return type and
            // modifiers", because Bash inherited Java's reason from the same fallback.
            for (word, truthful_of) in [
                // Markup may name a stylesheet: that is where the value belongs instead.
                (
                    "stylesheet",
                    &[
                        Language::Css,
                        Language::Scss,
                        Language::Sass,
                        Language::Html,
                        Language::Xml,
                    ][..],
                ),
                (
                    "CSS custom property",
                    &[
                        Language::Css,
                        Language::Scss,
                        Language::Sass,
                        Language::Html,
                        Language::Xml,
                    ][..],
                ),
                (
                    "markup",
                    &[Language::Html, Language::Xml, Language::Markdown][..],
                ),
                ("a method", &[Language::Java][..]),
                ("modifiers", &[Language::Java][..]),
                (
                    "return type",
                    &[Language::Java, Language::Rust, Language::Go, Language::Zig][..],
                ),
                ("package", &[Language::Java, Language::Go][..]),
            ] {
                assert!(
                    !reason.contains(word) || truthful_of.contains(language),
                    "{} for {language} says {word:?}, which describes {truthful_of:?} \
                     instead of this language: {reason}",
                    capability.label()
                );
            }
        }
    }
    assert!(
        examined > 100,
        "only {examined} reason(s) were examined; the matrix carries far more than that"
    );
}

#[test]
fn every_supported_cell_names_a_real_command() {
    for capability in Capability::ALL {
        assert!(
            capability.command().starts_with("fr "),
            "{} does not name a command",
            capability.label()
        );
    }
}

#[test]
fn nothing_is_merely_unimplemented() {
    // `Refused` means "could be done in this language, is not". Every such cell has
    // now been either built or shown to mean nothing in that language, so the matrix
    // should contain none at all.
    //
    // If this fails, a capability was added without deciding what it means everywhere,
    // which is how 27 unbuilt cells once came to be reported as complete.
    let mut refused: Vec<String> = Vec::new();
    for capability in Capability::ALL {
        for language in Language::ALL {
            if let Support::Refused { because } = capabilities::support(*capability, *language) {
                refused.push(format!("{} x {language}: {because}", capability.label()));
            }
        }
    }
    assert!(
        refused.is_empty(),
        "these cells are neither built nor explained away:\n  {}",
        refused.join("\n  ")
    );
}

#[test]
fn every_unsupported_cell_explains_itself() {
    for capability in Capability::ALL {
        for language in Language::ALL {
            let support = capabilities::support(*capability, *language);
            if let Support::NotApplicable { because } = support {
                assert!(
                    because.len() > 20,
                    "{} x {language} dismisses itself too briefly: {because:?}",
                    capability.label()
                );
            }
        }
    }
}

#[test]
fn config_languages_carry_their_share_of_the_mutations() {
    // The whole point of the tool: config and markup languages are not second-class.
    // If a change drops them below this, it is a regression worth noticing.
    let config = [
        Language::Hcl,
        Language::Helm,
        Language::Yaml,
        Language::Css,
        Language::Scss,
        Language::Markdown,
    ];
    let mutations = [
        Capability::Rename,
        Capability::SafeDelete,
        Capability::Restructure,
        Capability::ExtractVariable,
        Capability::InlineVariable,
    ];
    for language in config {
        for capability in mutations {
            assert!(
                capabilities::support(capability, language).is_yes(),
                "{} should serve {language}",
                capability.label()
            );
        }
    }
}

#[test]
fn the_published_totals_match_the_matrix() {
    // The table is checked and the sentence describing it was not, so the sentence drifted: the
    // README's rows counted 261 supported pairs while the line above them said 260. PLAN.md was
    // still quoting a total from before six capabilities and a language existed. A number
    // nobody checks is a table that lies in prose.
    let mut yes = 0usize;
    let mut rest = 0usize;
    for capability in Capability::ALL {
        for language in Language::ALL {
            match capabilities::support(*capability, *language) {
                Support::Yes => yes += 1,
                Support::NotApplicable { .. } | Support::Refused { .. } => rest += 1,
            }
        }
    }
    let total = yes + rest;

    // Each claim as it is written, with the numbers left as placeholders. A phrasing
    // that changes has to be changed here too, which is the point: the sentence and
    // the count are one thing.
    for (name, claims) in [
        (
            "README.md",
            &["YES of TOTAL capability × language pairs supported, REST not applicable"][..],
        ),
        (
            "docs/index.html",
            &["The tool supports YES of TOTAL capability × language pairs. It marks the other\n      REST"][..],
        ),
        (
            "docs/why.html",
            &["capability × language pairs marked \"refused\"; the tool marks the other REST not applicable"]
                [..],
        ),
        ("PLAN.md", &["YES of TOTAL capability ×"][..]),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        let text = std::fs::read_to_string(&path).expect("the file is readable");
        for claim in claims {
            let needle = claim
                .replace("YES", &yes.to_string())
                .replace("TOTAL", &total.to_string())
                .replace("REST", &rest.to_string());
            assert!(
                text.contains(&needle),
                "{name} does not say `{needle}`. The matrix has {yes} supported and \
                 {rest} not applicable out of {total}: regenerate the sentence as well \
                 as the table, or update the phrasing here if it changed."
            );
        }
    }
}
