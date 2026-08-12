//! A method that changes the thing it was called on.
//!
//! Four of these languages hand a method a reference and let it assign through it. Zig hands it
//! a value, and a value parameter there is const. So a method that assigns to a field is not a
//! slow method, it is a file that does not compile.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(file: &str, source: &str, to: Language) -> String {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(file);
    std::fs::write(&path, source).expect("the file");
    transpile::plan(&path, to).expect("a translation").output
}

const RUST: &str = "\
pub struct Counter {
    pub value: i64,
    pub step: i64,
}

impl Counter {
    pub fn bump(&mut self) {
        self.value = self.value + self.step;
    }

    pub fn peek(&self) -> i64 {
        return self.value;
    }
}
";

#[test]
fn a_zig_method_that_assigns_takes_a_pointer() {
    // The source said `&mut self`, the output said `self: Counter`, and the report said
    // every signature carried across with its types intact.
    let out = translated("a.rs", RUST, Language::Zig);
    assert!(out.contains("pub fn bump(self: *Counter) void"), "{out}");
}

#[test]
fn a_zig_method_that_only_reads_takes_a_value() {
    // A pointer everywhere would compile and would say something about the method that
    // is not true.
    let out = translated("a.rs", RUST, Language::Zig);
    assert!(out.contains("pub fn peek(self: Counter) i64"), "{out}");
}

#[test]
fn the_receiver_is_recognised_by_whatever_the_source_called_it() {
    // TypeScript says `this` and Python says `self`; the body carries the source's word
    // until it is written.
    let ts = translated(
        "a.ts",
        "export class Counter {\n    value: number;\n\n    bump() {\n        \
         this.value = this.value + 1;\n    }\n\n    peek(): number {\n        \
         return this.value;\n    }\n}\n",
        Language::Zig,
    );
    assert!(ts.contains("pub fn bump(self: *Counter) void"), "{ts}");
    assert!(ts.contains("pub fn peek(self: Counter)"), "{ts}");

    let py = translated(
        "a.py",
        "from dataclasses import dataclass\n\n@dataclass\nclass Box:\n    total: int\n\n    \
         def add(self, n: int):\n        self.total = self.total + n\n\n    \
         def get(self) -> int:\n        return self.total\n",
        Language::Zig,
    );
    assert!(py.contains("pub fn add(self: *Box, n: i64) void"), "{py}");
    assert!(py.contains("pub fn get(self: Box) i64"), "{py}");
}

#[test]
fn the_other_targets_are_unchanged() {
    // Java, TypeScript and Python hand a method a reference. Go already took a pointer receiver
    // for every method, which is safe and is what it did before.
    for (target, expected) in [
        (Language::Java, "public void bump()"),
        (Language::TypeScript, "bump()"),
        (Language::Go, "func (self *Counter) Bump()"),
    ] {
        let out = translated("a.rs", RUST, target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}
