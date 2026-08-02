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
fn refusals_are_distinguished_from_impossibilities() {
    // A `—` means "could be done, is not"; `n/a` means "means nothing here". Mixing
    // them is how a gap gets mistaken for a design decision — which is exactly what
    // happened when 27 unbuilt cells were reported as complete.
    let mut refused = 0;
    let mut not_applicable = 0;
    for capability in Capability::ALL {
        for language in Language::ALL {
            match capabilities::support(*capability, *language) {
                Support::Refused { because } => {
                    assert!(!because.is_empty());
                    refused += 1;
                }
                Support::NotApplicable { because } => {
                    assert!(!because.is_empty());
                    not_applicable += 1;
                }
                Support::Yes => {}
            }
        }
    }
    assert!(refused > 0 && not_applicable > 0, "both kinds should occur");
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
