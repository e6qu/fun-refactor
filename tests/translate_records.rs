//! Building a record, which is the line every constructor is made of.
//!
//! `Counter { value: 0, step }` is the one way Rust builds one, and nothing read it,
//! so every constructor body in every target came out as "not translated".

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(file: &str, source: &str, to: Language) -> String {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(file);
    std::fs::write(&path, source).expect("the file");
    transpile::plan(&path, to).expect("a translation").output
}

const POINT: &str = "\
pub struct Point {
    pub x: i64,
    pub y: i64,
}

pub fn origin() -> Point {
    return Point { x: 0, y: 0 };
}
";

#[test]
fn the_four_languages_that_name_their_fields_do() {
    for (target, expected) in [
        (Language::Python, "Point(x=0, y=0)"),
        (Language::Go, "Point{X: 0, Y: 0}"),
        (Language::Zig, "Point{ .x = 0, .y = 0 }"),
    ] {
        let out = translated("a.rs", POINT, target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}

#[test]
fn the_shorthand_is_read_as_the_field_it_names() {
    // `Point { x, y }` is `x: x, y: y`, and is how the code is actually written.
    let source = "\
pub struct Point {
    pub x: i64,
    pub y: i64,
}

pub fn at(x: i64, y: i64) -> Point {
    return Point { x, y };
}
";
    let out = translated("a.rs", source, Language::Python);
    assert!(out.contains("Point(x=x, y=y)"), "{out}");
}

const COUNTER: &str = "\
pub struct Counter {
    pub value: i64,
    pub step: i64,
}

impl Counter {
    pub fn new(step: i64) -> Counter {
        return Counter { value: 0, step: step };
    }
}
";

#[test]
fn a_constructor_that_builds_and_returns_keeps_its_body() {
    // Rust, Go and Zig have no constructor, only a function that returns the type. The
    // body was thrown away for all three under a rule about bodies that assign through
    // a receiver, which this one does not.
    for (target, expected) in [
        (Language::Go, "return Counter{Value: 0, Step: step}"),
        (Language::Zig, "return Counter{ .value = 0, .step = step };"),
    ] {
        let out = translated("a.rs", COUNTER, target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}

#[test]
fn a_constructor_becomes_field_assignments_where_one_takes_a_receiver() {
    // An `__init__` that returns a value raises; a Java constructor that returns one
    // does not compile. Building and returning the record says exactly what assigning
    // through the receiver says.
    let out = translated("a.rs", COUNTER, Language::Python);
    assert!(out.contains("self.value = 0"), "{out}");
    assert!(out.contains("self.step = step"), "{out}");
    assert!(!out.contains("return Counter("), "{out}");

    for target in [Language::Java, Language::TypeScript] {
        let out = translated("a.rs", COUNTER, target);
        assert!(out.contains("this.value = 0;"), "{target}:\n{out}");
        assert!(out.contains("this.step = step;"), "{target}:\n{out}");
    }
}

#[test]
fn an_enum_variant_is_not_a_record() {
    // `StopReason::Conditional { … }` builds a tagged union, which no target here has.
    // Writing the path through produced Go that says `StopReason::Conditional{…}`,
    // which Go does not parse, the round-trip sweep caught it.
    let source = "\
pub enum Stop {
    Conditional { what: String },
}

pub fn make(what: String) -> Stop {
    return Stop::Conditional { what: what };
}
";
    let out = translated("a.rs", source, Language::Go);
    assert!(!out.contains("Stop::Conditional{"), "{out}");
    assert!(out.contains("not translated"), "{out}");
}
