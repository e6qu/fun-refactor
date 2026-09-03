//! What `fr` tells a reader about the recipe language. Every list comes from the
//! code that reads a recipe, so it cannot drift.

use std::process::Command;

fn fr(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(args)
        .output()
        .expect("fr runs");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn the_vocabulary_names_every_verb_the_parser_reads() {
    let text = fr(&["recipe", "--vocabulary"]);
    for verb in [
        "rename",
        "delete",
        "move",
        "imports",
        "inline",
        "extract",
        "signature",
        "remove-flag",
        "restructure",
        "rewrite",
        "translate",
    ] {
        assert!(text.contains(verb), "`{verb}` is missing: {text}");
        assert!(
            fun_refactor::recipe::RESERVED.contains(&verb),
            "the vocabulary lists `{verb}` and the parser reserves no such word"
        );
    }
}

#[test]
fn the_vocabulary_names_every_recipe_requirement() {
    let text = fr(&["recipe", "--vocabulary"]);
    for requirement in fun_refactor::recipe::REQUIREMENTS {
        assert!(
            text.contains(requirement),
            "`{requirement}` is missing: the recipe guard is part of the language"
        );
    }
}

#[test]
fn the_vocabulary_names_every_recipe_expectation() {
    let text = fr(&["recipe", "--vocabulary"]);
    for expectation in fun_refactor::recipe::EXPECTATIONS {
        assert!(
            text.contains(expectation),
            "`{expectation}` is missing: a recipe's contract has to be knowable"
        );
    }
}

#[test]
fn the_vocabulary_names_every_predicate_and_says_which_a_file_takes() {
    let text = fr(&["recipe", "--vocabulary"]);
    for predicate in fun_refactor::recipe::PREDICATES {
        assert!(text.contains(predicate), "`{predicate}` is missing");
    }
    let (_, files) = text
        .split_once("acts on a file\n")
        .expect("the file predicates have their own heading");
    for predicate in fun_refactor::recipe::FILE_PREDICATES {
        assert!(
            files.contains(predicate),
            "`{predicate}` is missing from the file list"
        );
    }
}

#[test]
fn the_vocabulary_names_every_rewrite_this_build_has() {
    let text = fr(&["recipe", "--vocabulary"]);
    for rewrite in fun_refactor::refactor::rewrite::Rewrite::ALL {
        assert!(
            text.contains(rewrite.as_str()),
            "`{}` is missing: an agent cannot ask for what nothing names",
            rewrite.as_str()
        );
    }
}

#[test]
fn the_help_for_rewrite_names_every_rewrite() {
    // It hand-wrote three of the four, so `hoist-function` was unreachable for
    // anyone reading the help.
    let text = fr(&["rewrite", "--help"]);
    for rewrite in fun_refactor::refactor::rewrite::Rewrite::ALL {
        assert!(
            text.contains(rewrite.as_str()),
            "`fr rewrite --help` never names `{}`",
            rewrite.as_str()
        );
    }
}

#[test]
fn the_vocabulary_is_json_a_program_can_read() {
    let text = fr(&["--json", "recipe", "--vocabulary"]);
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("it is JSON");
    for key in [
        "schema",
        "requirements",
        "expectations",
        "verbs",
        "predicates",
        "file_predicates",
        "rewrites",
        "languages",
        "modifiers",
    ] {
        assert!(!parsed[key].is_null(), "`{key}` is missing from the JSON");
    }
    assert_eq!(
        parsed["requirements"].as_array().map(Vec::len),
        Some(fun_refactor::recipe::REQUIREMENTS.len()),
        "the JSON vocabulary drops a recipe guard"
    );
    assert_eq!(
        parsed["expectations"].as_array().map(Vec::len),
        Some(fun_refactor::recipe::EXPECTATIONS.len()),
        "the JSON vocabulary drops a recipe expectation"
    );
    let verbs = parsed["verbs"].as_array().expect("verbs is a list");
    assert_eq!(verbs.len(), 11, "the parser reads eleven operations");
    for verb in verbs {
        for field in ["name", "form", "acts_on", "selector"] {
            assert!(
                verb[field].is_string(),
                "a verb carries no `{field}`: {verb}"
            );
        }
    }
}

#[test]
fn every_language_the_vocabulary_offers_is_one_this_build_reads() {
    let text = fr(&["--json", "recipe", "--vocabulary"]);
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("it is JSON");
    let offered: Vec<String> = parsed["languages"]
        .as_array()
        .expect("languages is a list")
        .iter()
        .map(|l| l.as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(offered.len(), fun_refactor::lang::Language::ALL.len());
    for name in &offered {
        assert!(
            fun_refactor::lang::Language::from_name(name).is_some(),
            "the vocabulary lists `{name}` and it is not a language"
        );
    }
}
