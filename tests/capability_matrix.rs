//! The published matrix must match what the code does.

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
fn the_published_totals_match_the_matrix() {
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

    // Each claim as the document spells it, the numbers standing as placeholders.
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
        (
            "README.md",
            &["finds and changes code across N languages."][..],
        ),
        (
            "TUTORIAL.md",
            &["what each of the N languages supports"][..],
        ),
        ("EXAMPLES.md", &["across all WORD languages at once"][..]),
        (
            "PLAN.md",
            &[
                "The compile gate drives six of the WORD languages",
                "Build-out, in order: WORD languages",
            ][..],
        ),
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
fn the_status_table_in_the_plan_is_derived_from_the_code() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let plan = std::fs::read_to_string(root.join("PLAN.md")).expect("PLAN.md is readable");
    let bugs = std::fs::read_to_string(root.join("BUGS.md")).expect("BUGS.md is readable");

    let count_dir = |name: &str, keep: fn(&std::path::Path) -> bool| -> usize {
        std::fs::read_dir(root.join(name))
            .unwrap_or_else(|e| panic!("{name}/ is readable: {e}"))
            .filter_map(|entry| entry.ok())
            .filter(|entry| keep(&entry.path()))
            .count()
    };
    // `queries/` holds one directory per language and a README beside them.
    let query_sets = count_dir("queries", |p| p.is_dir());
    let catalogs = count_dir("catalogs", |p| {
        p.extension().and_then(|e| e.to_str()) == Some("yaml")
    });

    let mut supported = 0usize;
    let mut rest = 0usize;
    for capability in Capability::ALL {
        for language in Language::ALL {
            match capabilities::support(*capability, *language) {
                Support::Yes => supported += 1,
                Support::NotApplicable { .. } | Support::Refused { .. } => rest += 1,
            }
        }
    }

    let fixed = bugs.lines().filter(|l| l.starts_with("- [x] B")).count();
    let open = bugs.lines().filter(|l| l.starts_with("- [ ] B")).count();

    for row in [
        format!("| Query sets | {query_sets} |"),
        format!("| Entry-point catalogs | {catalogs} |"),
        format!(
            "| Capabilities × languages | {} × {} |",
            Capability::ALL.len(),
            Language::ALL.len()
        ),
        format!(
            "| Supported pairs | {supported} of {}, every other one carrying its reason |",
            supported + rest
        ),
        format!("| Defects fixed | {fixed} |"),
        format!("| Defects open | {open} |"),
    ] {
        assert!(
            plan.contains(&row),
            "PLAN.md's status table does not have the row `{row}`. Update the \
             table, or update the phrasing here if the row was reworded."
        );
    }
}
