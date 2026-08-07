//! Valid source the grammars cannot read.
//!
//! Each of these is accepted by the language's own reference implementation and produces
//! an error node here. They are recorded rather than worked around, and they are pinned
//! rather than merely written down, for two reasons pointing opposite ways: a grammar
//! upgrade that fixes one should be noticed and the entry retired, and a grammar that
//! starts reading one of these *without* an error node while still building the wrong
//! tree would be worse than the error — a wrong answer with nothing to say it is one.
//!
//! Every case here has a BUGS.md entry. When a test fails, the entry is what to update.

use fun_refactor::lang::Language;
use fun_refactor::parse::Parsers;

fn error_nodes(language: Language, source: &str) -> usize {
    Parsers::new()
        .parse(language, source)
        .expect("the grammar loads")
        .error_spans()
        .len()
}

#[test]
fn python_cannot_read_a_starred_literal_in_a_bare_tuple() {
    // B233. `g = 1, *[2]` is ordinary Python. A starred *name* or *call* in the same
    // position is read fine, so this is narrow — but `g = 1, *"ten"` appears in black's
    // own test data, which is where it was found.
    for source in [
        "g = 1, *[2]\n",
        "g = 1, *(2,)\n",
        "g = 1, *{2}\n",
        "g = 1, *\"ab\"\n",
        "g = *\"ab\", 1\n",
    ] {
        assert!(
            error_nodes(Language::Python, source) > 0,
            "the grammar now reads `{}` — retire B233 if the tree is right",
            source.trim()
        );
    }
}

#[test]
fn python_reads_the_forms_around_that_one() {
    // The boundary of B233, so a fix that over-corrects is visible too.
    for source in [
        "g = 1, *rest\n",
        "g = 1, *f()\n",
        "g = (1, *[2])\n",
        "g = [1, *[2]]\n",
        "a, *b = [1, 2, 3]\n",
        "f(1, *rest)\n",
    ] {
        assert_eq!(
            error_nodes(Language::Python, source),
            0,
            "`{}` should read cleanly",
            source.trim()
        );
    }
}

#[test]
fn python_cannot_read_a_type_parameter_default() {
    // B234. PEP 696, Python 3.13.
    assert!(error_nodes(Language::Python, "type A[T = int] = float\n") > 0);
    assert_eq!(
        error_nodes(Language::Python, "type A[T] = float\n"),
        0,
        "a type alias without a default should read cleanly"
    );
}

#[test]
fn typescript_cannot_read_an_import_type() {
    // B231.
    assert!(
        error_nodes(
            Language::TypeScript,
            "type A = { ast?: import(\"@babel/types\").Statement[] }\n"
        ) > 0
    );
}

#[test]
fn typescript_cannot_read_a_property_called_in_after_another() {
    // B232. Alone it is fine, which is what makes it worth pinning from both sides.
    assert!(
        error_nodes(
            Language::TypeScript,
            "interface G {\n  a?: string\n  in?: string\n}\n"
        ) > 0
    );
    assert_eq!(
        error_nodes(Language::TypeScript, "interface G {\n  in?: string\n}\n"),
        0,
        "`in` as the only member should read cleanly"
    );
}
