//! `IR.md` names every variant of the shared vocabulary.

use std::collections::BTreeSet;

const IR: &str = include_str!("../src/transpile/ir.rs");
const DOC: &str = include_str!("../IR.md");

/// The variant names declared by `pub enum <name>` in `ir.rs`.
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

/// The document with every run of whitespace flattened to one space. A claim about a
/// sentence should survive the day someone rewraps the paragraph it sits in.
fn flat() -> String {
    DOC.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Does the document name this variant, spelled as code?
fn names(variant: &str) -> bool {
    // `Named { name, args }`, `List(T)`, `` `Set` ``: the shape or the closing backtick follows
    // the name.
    DOC.contains(&format!("`{variant}`"))
        || DOC.contains(&format!("`{variant}("))
        || DOC.contains(&format!("`{variant} {{"))
        || DOC.contains(&format!("`{variant}<"))
}

fn assert_documented(enum_name: &str) {
    let all = variants_of(enum_name);
    assert!(
        all.len() > 2,
        "only {} variant(s) came out of `{enum_name}`, so this checked \
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
    // The document opens each section with a count.
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
fn every_language_the_middle_touches_is_named() {
    // The document opens by listing them, on both sides.
    for language in fun_refactor::transpile::READABLE
        .iter()
        .chain(fun_refactor::transpile::WRITABLE)
    {
        // `Language`'s own `Display` is the lowercase name the CLI takes.
        let title = match format!("{language}").as_str() {
            "typescript" => "TypeScript".to_string(),
            name => format!("{}{}", name[..1].to_uppercase(), &name[1..]),
        };
        assert!(
            DOC.contains(&title),
            "IR.md never names {title:?}, which the middle reads or writes."
        );
    }
}

/// A number as this document writes it. The prose spells them out, so a test that
/// checks the prose has to as well.
fn in_words(n: usize) -> String {
    const UNITS: [&str; 20] = [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TENS: [&str; 10] = [
        "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    assert!(n < 100, "IR.md counts nothing this large");
    match (n, n % 10) {
        (n, _) if n < 20 => UNITS[n].to_string(),
        (n, 0) => TENS[n / 10].to_string(),
        (n, unit) => format!("{}-{}", TENS[n / 10], UNITS[unit]),
    }
}

/// The same word, opening a sentence.
fn capitalised(word: &str) -> String {
    format!("{}{}", word[..1].to_uppercase(), &word[1..])
}

#[test]
fn the_counts_of_readers_writers_and_pairs_are_right() {
    // The opening paragraph states all three, and each moves when a language gains a
    // reader or a writer.
    let readers = fun_refactor::transpile::READABLE.len();
    let writers = fun_refactor::transpile::WRITABLE.len();
    // Every readable source against every writable target that is not itself.
    let pairs: usize = fun_refactor::transpile::READABLE
        .iter()
        .map(|from| {
            fun_refactor::transpile::WRITABLE
                .iter()
                .filter(|to| *to != from)
                .count()
        })
        .sum();
    let doc = flat();
    for claim in [
        format!(
            "{} languages have a reader",
            capitalised(&in_words(readers))
        ),
        format!("{} have a writer", capitalised(&in_words(writers))),
        format!(
            "{} ordered pairs go through one vocabulary.",
            capitalised(&in_words(pairs))
        ),
        format!(
            "{} pairs need {} of each",
            capitalised(&in_words(pairs)),
            in_words(pairs)
        ),
        format!(
            "A middle costs {} readers and {} writers",
            in_words(readers),
            in_words(writers)
        ),
    ] {
        assert!(
            doc.contains(&claim),
            "IR.md does not say {claim:?}. {readers} language(s) have a reader and \
             {writers} have a writer, giving {pairs} ordered pairs."
        );
    }
}
