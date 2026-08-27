//! `IR.md` names every variant of the shared vocabulary.
//!
//! The reference is only worth reading while it is complete. A variant added to
//! `ir.rs` and left out of the document is a construct nobody knows crosses,
//! and a variant removed while the document keeps describing it sends a reader
//! looking for something that is gone.
//!
//! `src/transpile/ir.rs` is the authority, read as text. Nothing else in the
//! repository knows the full list, since the enums have no reflection.

use std::collections::BTreeSet;

const IR: &str = include_str!("../src/transpile/ir.rs");
const DOC: &str = include_str!("../IR.md");

/// The variant names declared by `pub enum <name>` in `ir.rs`.
///
/// A variant sits at four spaces of indent inside the enum, and starts with a
/// capital. Doc comments, attributes and the fields of a struct-like variant
/// all fail one of those, so none of them is mistaken for a variant.
fn variants_of(enum_name: &str) -> BTreeSet<String> {
    let head = format!("pub enum {enum_name} {{");
    let start = IR
        .find(&head)
        .unwrap_or_else(|| panic!("`{head}` is in ir.rs"))
        + head.len();
    let mut found = BTreeSet::new();
    let mut depth = 1usize;
    for line in IR[start..].lines() {
        depth += line.matches('{').count();
        depth -= line.matches('}').count().min(depth);
        if depth == 0 {
            break;
        }
        let Some(rest) = line.strip_prefix("    ") else {
            continue;
        };
        if rest.starts_with(' ') {
            continue;
        }
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            found.insert(name);
        }
    }
    found
}

/// Does the document name this variant, spelled as code?
///
/// The name has to appear inside backticks. `Set` and `Map` are ordinary words,
/// and a check for the bare text would pass on a sentence that never meant the
/// variant.
fn names(variant: &str) -> bool {
    // `Named { name, args }`, `List(T)`, `` `Set` ``: the name is followed by
    // its shape or by the closing backtick.
    DOC.contains(&format!("`{variant}`"))
        || DOC.contains(&format!("`{variant}("))
        || DOC.contains(&format!("`{variant} {{"))
        || DOC.contains(&format!("`{variant}<"))
}

fn assert_documented(enum_name: &str) {
    let all = variants_of(enum_name);
    assert!(
        all.len() > 2,
        "only {} variant(s) were read out of `{enum_name}`, so this checked \
         almost nothing: {all:?}",
        all.len()
    );
    let missing: Vec<&String> = all.iter().filter(|v| !names(v)).collect();
    assert!(
        missing.is_empty(),
        "IR.md never names {missing:?} of `{enum_name}`. A construct the \
         reference omits is one nobody knows crosses."
    );
}

#[test]
fn every_item_is_documented() {
    assert_documented("Item");
}

#[test]
fn every_statement_is_documented() {
    assert_documented("Stmt");
}

#[test]
fn every_expression_is_documented() {
    assert_documented("Expr");
}

#[test]
fn every_type_is_documented() {
    assert_documented("Type");
}

#[test]
fn every_operator_is_documented() {
    assert_documented("BinaryOp");
    assert_documented("UnaryOp");
}

#[test]
fn every_field_of_the_report_is_documented() {
    // The report is what a reader acts on, so an undocumented field is a number
    // in the output with nothing saying what it counts.
    let head = "pub struct Fidelity {";
    let start = IR.find(head).expect("`Fidelity` is in ir.rs") + head.len();
    let body = &IR[start..];
    let end = body.find("\n}").expect("the struct closes");
    let missing: Vec<String> = body[..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub "))
        .filter_map(|l| l.split(':').next())
        .map(|n| n.trim().to_string())
        .filter(|n| !DOC.contains(&format!("`{n}`")))
        .collect();
    assert!(
        missing.is_empty(),
        "IR.md documents no meaning for the `Fidelity` field(s) {missing:?}."
    );
}

#[test]
fn the_counts_the_document_states_are_right() {
    // The document opens each section with a count. A number stated in prose
    // rots faster than anything else in a reference.
    for (enum_name, stated) in [
        ("Item", "Nine kinds of top-level thing."),
        ("Stmt", "Twenty-six."),
        ("Expr", "Twenty-nine."),
        ("Type", "Twelve types."),
    ] {
        let n = variants_of(enum_name).len();
        let word = match n {
            9 => "Nine",
            12 => "Twelve",
            26 => "Twenty-six",
            29 => "Twenty-nine",
            other => panic!(
                "`{enum_name}` has {other} variants, and this test has no word \
                 for that number. Update IR.md and this list together."
            ),
        };
        assert!(
            stated.starts_with(word),
            "`{enum_name}` has {n} variants, and IR.md says {stated:?}."
        );
        assert!(
            DOC.contains(stated),
            "IR.md no longer says {stated:?}, so the count it states went \
             unchecked."
        );
    }
}

#[test]
fn every_language_with_a_writer_is_named() {
    // The document opens by listing them. A language gaining a writer without
    // reaching this list is one nobody reads about.
    for language in fun_refactor::transpile::SUPPORTED {
        // `Language`'s own `Display` is the lowercase name the CLI takes.
        // The document writes prose, so it capitalises, and TypeScript
        // capitalises twice.
        let title = match format!("{language}").as_str() {
            "typescript" => "TypeScript".to_string(),
            name => format!("{}{}", name[..1].to_uppercase(), &name[1..]),
        };
        assert!(
            DOC.contains(&title),
            "IR.md never names {title:?}, which has a reader and a writer."
        );
    }
}

#[test]
fn the_counts_of_readers_and_pairs_are_right() {
    // The opening paragraph states both, and both move when a language gains a
    // reader and a writer. Bash was the seventh, and the pairs went from thirty
    // to forty-two in the same commit.
    let n = fun_refactor::transpile::SUPPORTED.len();
    let pairs = n * (n - 1);
    let (word, pairs_word) = match (n, pairs) {
        (6, 30) => ("Six", "Thirty"),
        (7, 42) => ("Seven", "Forty-two"),
        (8, 56) => ("Eight", "Fifty-six"),
        _ => panic!(
            "{n} language(s) have a reader and a writer, giving {pairs} ordered \
             pairs, and this test has no words for those numbers."
        ),
    };
    for claim in [
        format!("{word} languages\nhave a reader and a writer"),
        format!("{pairs_word} ordered pairs go through one vocabulary."),
        format!(
            "{word} languages need\n{} of them.",
            pairs_word.to_lowercase()
        ),
        format!("A middle costs {} of each", word.to_lowercase()),
    ] {
        assert!(
            DOC.contains(&claim),
            "IR.md does not say {claim:?}. {n} language(s) have a reader and a \
             writer, giving {pairs} ordered pairs."
        );
    }
}
