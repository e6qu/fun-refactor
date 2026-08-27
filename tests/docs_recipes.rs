//! `RECIPES.md` documents every word the recipe parser accepts.

use std::collections::BTreeSet;

const PARSER: &str = include_str!("../src/recipe/parse.rs");
const DOC: &str = include_str!("../RECIPES.md");

/// Every lowercase word the parser compares an input token against.
fn keywords() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = PARSER;
    while let Some(open) = rest.find('"') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        let text = &after[..close];
        rest = &after[close + 1..];
        let word = text.len() >= 3
            && text.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            && !text.starts_with('-')
            && !text.ends_with('-');
        if word {
            found.insert(text.to_string());
        }
    }
    found
}

#[test]
fn every_word_the_parser_takes_is_documented() {
    let missing: Vec<String> = keywords()
        .into_iter()
        .filter(|word| !DOC.contains(word.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "RECIPES.md never mentions {missing:?}, which the recipe parser accepts. \
         A recipe is written by hand, so a word the document omits is one nobody \
         can use."
    );
}

#[test]
fn the_check_read_the_vocabulary() {
    // A literal-finder that broke would pass the check above against nothing.
    let found = keywords();
    assert!(
        found.len() > 15,
        "only {} word(s) were read out of the parser. The check above compared \
         almost nothing: {found:?}.",
        found.len()
    );
}

#[test]
fn every_verb_is_in_the_grammar_and_the_table() {
    // The verbs are the shape of the language.
    let head = PARSER
        .find("fn parse_step")
        .or_else(|| PARSER.find("fn step"))
        .expect("the parser dispatches on a verb somewhere");
    let body = &PARSER[head..];
    let verbs = [
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
    ];
    let unknown: Vec<&&str> = verbs
        .iter()
        .filter(|v| !body.contains(&format!("\"{v}\"")))
        .collect();
    assert!(
        unknown.is_empty(),
        "this test lists {unknown:?} as verbs and the parser dispatches on no \
         such word. Update the list, or the verb was renamed."
    );

    let grammar = {
        let start = DOC
            .find("operation   = ")
            .expect("the grammar states the operations");
        let end = DOC[start..].find("\n\n").expect("the production ends") + start;
        &DOC[start..end]
    };
    let ungrammared: Vec<&&str> = verbs
        .iter()
        .filter(|v| !grammar.contains(&format!("\"{v}\"")))
        .collect();
    assert!(
        ungrammared.is_empty(),
        "the grammar in RECIPES.md has no production for {ungrammared:?}."
    );

    let untabled: Vec<&&str> = verbs
        .iter()
        .filter(|v| !DOC.contains(&format!("| `{v}` |")))
        .collect();
    assert!(
        untabled.is_empty(),
        "the operations table in RECIPES.md has no row for {untabled:?}, so \
         nothing says what they take."
    );
}
