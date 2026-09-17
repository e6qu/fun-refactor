//! The published matrix must match what the code does.

use fun_refactor::capabilities::{self, Capability, Support};
use fun_refactor::lang::Language;

#[test]
fn the_readme_directs_readers_to_the_live_matrix() {
    let readme = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/README.md"))
        .expect("README.md is readable");
    assert!(readme.contains("fr capabilities"));
    assert!(readme.contains("fr --json audit"));
    assert!(!readme.contains("| symbols/def/refs |"));
}

#[test]
fn every_command_that_has_a_per_language_answer_is_in_the_matrix() {
    // The matrix is the tool's own claim about what it does, per language.
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
    let mut examined = 0;
    for capability in Capability::ALL {
        for language in Language::ALL {
            let Some(reason) = capabilities::support(*capability, *language).reason() else {
                continue;
            };
            examined += 1;
            // A word only some languages can be described with, and the ones that can.
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
    // `Refused` means "could be done in this language, is not".
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
fn the_website_uses_the_live_matrix_instead_of_copied_totals() {
    for name in ["docs/index.html", "docs/why.html"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        let text = std::fs::read_to_string(&path).expect("the file is readable");
        assert!(
            text.contains("fr capabilities"),
            "{name} must name the live matrix"
        );
        assert!(!text.contains("capability × language pairs marked"));
        assert!(!text.contains("The tool supports 311 of 456"));
    }
}

#[test]
fn the_published_language_count_matches_the_list() {
    // The same failure as above, in the other number the docs state.
    let n = Language::ALL.len();
    let word = match n {
        17 => "seventeen",
        18 => "eighteen",
        19 => "nineteen",
        other => panic!(
            "this tool reads {other} languages and this test has no word for that \
             number. Add it here and update the prose that states it."
        ),
    };
    for (name, claims) in [
        ("README.md", &["supports N parser identities"][..]),
        (
            "TUTORIAL.md",
            &["what each of the N languages supports"][..],
        ),
        ("EXAMPLES.md", &["across all WORD languages at once"][..]),
        (
            "docs/index.html",
            &[
                "A multi-language refactoring tool for N languages.",
                "N languages · one index",
            ][..],
        ),
        (
            "docs/why.html",
            &["one index across WORD languages out of syntax alone."][..],
        ),
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
        let text = std::fs::read_to_string(&path).expect("the file is readable");
        for claim in claims {
            let needle = claim.replace("WORD", word).replace('N', &n.to_string());
            assert!(
                text.contains(&needle),
                "{name} does not say `{needle}`. This tool reads {n} language(s): \
                 update the sentence, or update the phrasing here if it changed."
            );
        }
    }
}

#[test]
fn the_plan_uses_live_audits_instead_of_copied_support_counts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let plan = std::fs::read_to_string(root.join("PLAN.md")).expect("PLAN.md is readable");
    assert!(plan.contains("fr --json audit"));
    assert!(plan.contains("fr capabilities"));
    assert!(!plan.contains("| Fixed defects |"));
    assert!(!plan.contains("| Open defects |"));
}
