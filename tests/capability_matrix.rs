//! The published matrix must match what the code actually does.
//!
//! This exists because the README's matrix drifted once already: capabilities gated
//! by an explicit predicate stayed accurate while ones left to emerge from grammar
//! shape did not, and `inline --call` was documented for six languages while working
//! for two. A table nobody checks is a table that lies.

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
fn every_supported_cell_names_a_real_command() {
    for capability in Capability::ALL {
        assert!(
            capability.command().starts_with("fr "),
            "{} does not name a command",
            capability.as_str()
        );
    }
}

#[test]
fn nothing_is_merely_unimplemented() {
    // `Refused` means "could be done in this language, is not". Every such cell has
    // now been either built or shown to mean nothing in that language, so the matrix
    // should contain none at all.
    //
    // If this fails, a capability was added without deciding what it means everywhere
    // — which is exactly how 27 unbuilt cells once came to be reported as complete.
    let mut refused: Vec<String> = Vec::new();
    for capability in Capability::ALL {
        for language in Language::ALL {
            if let Support::Refused { because } = capabilities::support(*capability, *language) {
                refused.push(format!("{} x {language}: {because}", capability.as_str()));
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
                    capability.as_str()
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
                capability.as_str()
            );
        }
    }
}
