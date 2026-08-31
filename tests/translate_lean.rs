//! Writing the shared representation as Lean 4.
//!
//! The conformance suite runs what this writes and compares the transcript. Here are
//! the decisions that suite cannot see: the ones where Lean accepts several spellings
//! and one of them means what the source meant.

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn lean(name: &str, source: &str) -> String {
    let (_tmp, root) = common::tree(&[(name, source)]);
    transpile::plan(&root.join(name), Language::Lean)
        .unwrap_or_else(|e| panic!("{name} -> lean: {e}"))
        .output
}

#[test]
fn lean_is_read_and_written_both() {
    assert!(
        transpile::can_be_written(Language::Lean),
        "a file becomes Lean"
    );
    assert!(
        transpile::can_be_read(Language::Lean),
        "and a Lean file becomes something else"
    );
    assert!(
        transpile::WRITABLE.contains(&Language::Lean)
            && transpile::READABLE.contains(&Language::Lean),
        "and both lists say so"
    );
}

#[test]
fn each_declaration_takes_the_form_lean_has_for_it() {
    let source = "\
pub struct Reading {
    pub sensor: String,
    pub celsius: f64,
}

pub enum Shape {
    Empty,
    Circle { radius: f64 },
}

pub fn double(n: i64) -> i64 {
    return n * 2;
}
";
    let out = lean("a.rs", source);
    for wanted in [
        "structure Reading where",
        "sensor : String",
        "celsius : Float",
        "deriving Repr, Inhabited, BEq",
        "inductive Shape where",
        "| empty",
        "| circle (radius : Float)",
        "def double (n : Int) : Int :=",
    ] {
        assert!(out.contains(wanted), "missing `{wanted}`:\n{out}");
    }
}

#[test]
fn a_distinct_type_becomes_an_abbreviation_and_says_why() {
    // `abbrev` and not a type of its own. A distinct type over `Int` needs its own
    // arithmetic and coercions, and the source declared none.
    let source = "from typing import NewType\n\nMeters = NewType(\"Meters\", int)\n";
    let (_tmp, root) = common::tree(&[("a.py", source)]);
    let plan = transpile::plan(&root.join("a.py"), Language::Lean).expect("a translation");
    assert!(
        plan.output.contains("abbrev Meters := Int"),
        "got:\n{}",
        plan.output
    );
    assert!(
        plan.fidelity
            .notes
            .iter()
            .any(|n| n.contains("abbreviation in Lean")),
        "the note says what an abbreviation is not. {:?}",
        plan.fidelity.notes
    );
}

#[test]
fn division_names_the_rounding_it_wants() {
    // Lean's `/` on `Int` rounds toward negative infinity and its `%` is the Euclidean
    // remainder. A source whose `/` truncates would compute something else under either.
    let source = "pub fn f(a: i64, b: i64) -> i64 {\n    return a / b + a % b;\n}\n";
    let out = lean("a.rs", source);
    assert!(out.contains("Int.tdiv a b"), "got:\n{out}");
    assert!(out.contains("Int.tmod a b"), "got:\n{out}");
    let arithmetic: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("return") && (l.contains(" / ") || l.contains(" % ")))
        .collect();
    assert!(
        arithmetic.is_empty(),
        "no bare operator should carry integer division. {arithmetic:?}"
    );
}

#[test]
fn a_binding_is_mutable_only_where_something_writes_to_it() {
    // Lean warns about a `let mut` nothing assigns to, and refuses an assignment to a
    // binding that is not one.
    let source = "\
pub fn f() -> i64 {
    let fixed = 1;
    let mut moving = 2;
    moving = moving + fixed;
    return moving;
}
";
    let out = lean("a.rs", source);
    assert!(out.contains("let fixed : Int := 1"), "got:\n{out}");
    assert!(out.contains("let mut moving : Int := 2"), "got:\n{out}");
}

#[test]
fn a_whole_number_binding_says_it_is_an_integer() {
    // Lean reads a bare `0` as a `Nat`, whose subtraction stops at zero. A counter that
    // crossed without its type would count differently below zero.
    let source = "pub fn f() -> i64 {\n    let n = 0;\n    return n - 1;\n}\n";
    let out = lean("a.rs", source);
    assert!(out.contains("let n : Int := 0"), "got:\n{out}");
}

#[test]
fn a_declaration_comes_after_everything_it_names() {
    // Lean reads a file once, top to bottom. No other target here cares what order the
    // declarations arrive in.
    let source = "\
pub fn caller() -> i64 {
    return callee();
}

pub fn callee() -> i64 {
    return 1;
}
";
    let out = lean("a.rs", source);
    let callee = out.find("def callee").expect("the callee is written");
    let caller = out.find("def caller").expect("the caller is written");
    assert!(
        callee < caller,
        "the file has to declare `callee` before `caller` reads it:\n{out}"
    );
}

#[test]
fn a_cycle_of_functions_goes_inside_mutual() {
    // Two functions that call each other cannot each come after the other, and `mutual`
    // is the only form Lean has for that.
    let source = "\
pub fn even(n: i64) -> bool {
    if n == 0 {
        return true;
    }
    return odd(n - 1);
}

pub fn odd(n: i64) -> bool {
    if n == 0 {
        return false;
    }
    return even(n - 1);
}
";
    let out = lean("a.rs", source);
    let start = out.find("mutual").unwrap_or_else(|| panic!("got:\n{out}"));
    let end = out.find("\nend").unwrap_or_else(|| panic!("got:\n{out}"));
    let inside = &out[start..end];
    assert!(inside.contains("def even"), "got:\n{out}");
    assert!(inside.contains("def odd"), "got:\n{out}");
}

#[test]
fn a_recursive_function_says_it_is_partial() {
    // Lean asks a recursive `def` to show that it stops. `partial` is the answer that
    // asks for a default value of the answer instead of a proof.
    let source = "\
pub fn countdown(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    return countdown(n - 1);
}
";
    let out = lean("a.rs", source);
    assert!(out.contains("partial def countdown"), "got:\n{out}");
}

#[test]
fn a_function_that_fails_answers_in_io() {
    // Lean's `panic!` answers with the type's default value and carries on. A failure
    // a caller means to catch leaves through a monad.
    let source = "\
pub fn check(n: i64) -> Result<i64, String> {
    if n < 0 {
        return Err(\"negative\".to_string());
    }
    Ok(n)
}
";
    let out = lean("a.rs", source);
    assert!(out.contains("def check (n : Int) : IO Int"), "got:\n{out}");
    assert!(
        out.contains("throw (IO.userError \"negative\")"),
        "got:\n{out}"
    );
}

#[test]
fn a_method_lands_in_the_namespace_its_structure_opens() {
    // `p.area` reaches `Shape.area` by that namespace alone, which is how a method call
    // crosses without a word changing at the call site.
    let source = "\
pub struct Counter {
    pub value: i64,
}

impl Counter {
    pub fn doubled(&self) -> i64 {
        return self.value * 2;
    }
}
";
    let out = lean("a.rs", source);
    assert!(
        out.contains("def Counter.doubled (self : Counter) : Int"),
        "got:\n{out}"
    );
}

#[test]
fn a_while_condition_is_bracketed() {
    // Lean applies a function by writing its argument beside it. A bare
    // `while i < 3 do` reads the `do` block as an argument of `3`.
    let source = "\
pub fn f() -> i64 {
    let mut i = 0;
    while i < 3 {
        i = i + 1;
    }
    return i;
}
";
    let out = lean("a.rs", source);
    assert!(out.contains("while (i < 3) do"), "got:\n{out}");
}

#[test]
fn a_choice_between_literals_is_a_chain_and_not_a_match() {
    // Lean matches a value against the constructors of its type. A `Float` has none, and
    // the numbers a `switch` selects on are not constructors of anything.
    let source = "\
pub fn name(day: i64) -> String {
    match day {
        1 => return \"mon\".to_string(),
        2 => return \"tue\".to_string(),
        _ => return \"other\".to_string(),
    }
}
";
    let out = lean("a.rs", source);
    assert!(out.contains("if day == 1 then"), "got:\n{out}");
    assert!(out.contains("if day == 2 then"), "got:\n{out}");
    assert!(!out.contains("match day with"), "got:\n{out}");
    // Each arm inside the `else` of the one before it. Not `else if` on one line:
    // B832 says the grammar cannot read that.
    assert!(!out.contains("else if"), "got:\n{out}");
}

#[test]
fn a_deferred_block_runs_at_the_end_of_the_scope() {
    // Go runs a deferred call when the function returns. Written where it stands, it
    // would run in the middle.
    let source = "\
package main

import \"fmt\"

func work() {
\tfmt.Println(\"open\")
\tdefer fmt.Println(\"close\")
\tfmt.Println(\"work\")
}
";
    let out = lean("a.go", source);
    let close = out
        .find("\"close\"")
        .unwrap_or_else(|| panic!("got:\n{out}"));
    let work = out
        .find("\"work\"")
        .unwrap_or_else(|| panic!("got:\n{out}"));
    assert!(
        work < close,
        "the deferred line runs after the rest of the scope.\n{out}"
    );
}

#[test]
fn a_fraction_prints_the_way_every_other_target_prints_one() {
    // Lean's own `toString` writes six decimal places, so a transcript that agrees
    // everywhere else would disagree here for the formatting alone.
    let source = "\
function show(n: number): void {
  console.log(`n ${n}`);
}
";
    let out = lean("a.ts", source);
    assert!(
        out.contains("def frShow (value : Float) : String"),
        "got:\n{out}"
    );
    assert!(out.contains("{frShow n}"), "got:\n{out}");
}

#[test]
fn the_helpers_come_before_the_declarations_that_call_them() {
    let source = "function show(n: number): void {\n  console.log(`n ${n}`);\n}\n";
    let out = lean("a.ts", source);
    let helper = out.find("def frShow").expect("the helper is written");
    let user = out.find("def show").expect("the function is written");
    assert!(helper < user, "got:\n{out}");
}

#[test]
fn what_lean_cannot_spell_is_in_the_output_and_counted() {
    let source = "\
pub fn shout(names: Vec<String>) -> Vec<String> {
    let loud: Vec<String> = names.iter().map(|n| { n.to_uppercase() }).collect();
    return loud;
}
";
    let (_tmp, root) = common::tree(&[("a.rs", source)]);
    let plan = transpile::plan(&root.join("a.rs"), Language::Lean).expect("a translation");
    assert!(plan.fidelity.carried_verbatim > 0, "got:\n{}", plan.output);
    assert!(
        plan.output.contains(transpile::MARKER),
        "carried code must be marked:\n{}",
        plan.output
    );
    // And the signature still crossed, which is the point of translating at all.
    assert!(
        plan.output
            .contains("def shout (names : Array String) : Array String"),
        "got.\n{}",
        plan.output
    );
}

#[test]
fn a_comment_in_lean_opens_with_two_dashes() {
    let source = "pub fn f() -> i64 {\n    return 1;\n}\n";
    let out = lean("a.rs", source);
    let header = out.lines().next().unwrap_or_default();
    assert!(
        header.starts_with("-- Translated from rust"),
        "got: {header:?}"
    );
    assert!(
        !out.contains("// "),
        "a `//` in Lean is not a comment:\n{out}"
    );
}

#[test]
fn a_value_written_where_a_return_would_go_is_the_answer() {
    // A `do` block answers with its last element. Naming that value and dropping it
    // would compile in Lean and answer with the wrong thing.
    let source = "\
pub fn for_display(celsius: f64, metric: bool) -> f64 {
    if metric {
        celsius
    } else {
        celsius * 2.0
    }
}
";
    let out = lean("a.rs", source);
    assert!(out.contains("    celsius\n"), "got:\n{out}");
    assert!(!out.contains("let _ := celsius"), "got:\n{out}");
}

#[test]
fn a_record_with_no_fields_is_a_structure_with_no_fields() {
    // A Java class carrying only methods has no fields to declare. The constructor
    // line a `structure` can take is one the grammar will not read.
    let source = "\
public class E {
    public int add(int a, int b) {
        return a + b;
    }
}
";
    let out = lean("E.java", source);
    assert!(out.contains("structure E where\nderiving"), "got:\n{out}");
    assert!(out.contains("def E.add (self : E)"), "got:\n{out}");
}

#[test]
fn a_parameter_that_shadows_a_function_makes_no_cycle() {
    // `fn celsius(fahrenheit: f64)` reads `fahrenheit` and means its own parameter.
    // Taking it for the function beside it invents a cycle. A cycle costs a `mutual`
    // block claiming the two depend on each other.
    let source = "\
pub fn fahrenheit(celsius: f64) -> f64 {
    return celsius * 2.0;
}

pub fn celsius(fahrenheit: f64) -> f64 {
    return fahrenheit / 2.0;
}
";
    let out = lean("a.rs", source);
    assert!(!out.contains("mutual"), "neither calls the other:\n{out}");
}

#[test]
fn a_body_that_carried_everything_still_answers() {
    // A `do` with nothing in it is a syntax error. A `do` ending in a comment leaves
    // the layout open, so the next declaration lands inside it.
    let source = "\
def totals(readings):
    with open(readings) as f:
        return f.read()
";
    let out = lean("a.py", source);
    assert!(out.contains(transpile::MARKER), "got:\n{out}");
    let body: Vec<&str> = out
        .lines()
        .skip_while(|l| !l.starts_with("def totals"))
        .skip(1)
        .filter(|l| !l.trim().is_empty())
        .collect();
    assert!(
        body.last()
            .is_some_and(|l| !l.trim_start().starts_with("--")),
        "the block has to end on something Lean reads as an element. {body:?}"
    );
}

/// Every word the grammar refuses in an identifier position is one the writer escapes.
///
/// A field called `prefix` is the case that found this. `prefix` opens a notation
/// declaration in Lean, so `prefix : String` swallowed the `deriving` line after it.
/// The whole translation then refused at the reparse gate.
#[test]
fn the_reserved_words_are_the_ones_the_grammar_refuses() {
    use fun_refactor::lang::Language;
    use fun_refactor::parse::Parsers;

    // Every literal in the grammar that is shaped like an identifier. Most are field
    // names rather than keywords, and asking is what tells the two apart.
    let grammar = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/grammars/lean/grammar.js"
    ))
    .expect("the grammar is readable");
    let mut candidates: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    for c in grammar.chars() {
        match quote {
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                word.clear();
            }
            Some(q) if c == q => {
                quote = None;
                let identifier = !word.is_empty()
                    && word.starts_with(|c: char| c.is_ascii_lowercase())
                    && word.chars().all(|c| c.is_ascii_lowercase() || c == '_');
                if identifier {
                    candidates.push(std::mem::take(&mut word));
                }
            }
            Some(_) => word.push(c),
            None => {}
        }
    }
    candidates.sort();
    candidates.dedup();
    assert!(
        candidates.len() > 60,
        "the grammar should hold plenty of literals, and this found {}",
        candidates.len()
    );

    let parsers = Parsers::new();
    let mut missing = Vec::new();
    for word in &candidates {
        // One file using the word where an identifier goes, three ways over.
        let source = format!(
            "structure S where\n  {word} : String\nderiving Repr\n\n\
             def f{word} : Int := Id.run do\n  let {word} := 1\n  return {word}\n\n\
             def g{word} ({word} : Int) : Int := {word}\n"
        );
        let refused = parsers
            .parse(Language::Lean, &source)
            .expect("the grammar loads")
            .has_errors();
        if refused && !transpile_reserves(word) {
            missing.push(word.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "the grammar refuses these where an identifier goes, and the writer does not \
         escape them: {missing:?}"
    );
}

/// Does the writer escape this word? Asked through the output, since the list itself is
/// private to the writer.
fn transpile_reserves(word: &str) -> bool {
    let source = format!("pub struct S {{\n    pub {word}: String,\n}}\n");
    let out = lean("a.rs", &source);
    !out.contains(&format!("\n  {word} : String"))
}
