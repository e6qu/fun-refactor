//! Python's `/` and C's `/` are different operations that share a spelling.
//!
//! Read as the same one, `self.cents / 100` became an integer division in every
//! target whose `/` is C's. Rust and Go refused the file. Java took it and
//! answered 5 where the source answered 5.34, which is the worse of the two.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(source: &str, name: &str, target: Language) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("out.txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

const ACCOUNT: &str = "class Account:\n    def __init__(self, cents: int):\n        \
    self.cents = cents\n\n    def deposit(self, amount: int) -> None:\n        \
    self.cents += amount\n\n    def balance(self) -> float:\n        \
    return self.cents / 100\n";

#[test]
fn rust_divides_in_floats_and_takes_a_writable_receiver() {
    let rust = translated(ACCOUNT, "acc.py", Language::Rust);
    assert!(
        rust.contains("self.cents as f64 / 100.0"),
        "a float division divides floats.\n{rust}"
    );
    assert!(
        rust.contains("fn deposit(&mut self"),
        "a body that writes a field needs a receiver it can write.\n{rust}"
    );
    assert!(
        rust.contains("fn balance(&self"),
        "a body that only reads keeps the shared borrow.\n{rust}"
    );
}

#[test]
fn go_divides_in_floats() {
    let go = translated(ACCOUNT, "acc.py", Language::Go);
    assert!(
        go.contains("float64(self.Cents) / 100.0"),
        "Go's own `/` would truncate.\n{go}"
    );
}

#[test]
fn java_divides_in_floats() {
    let java = translated(ACCOUNT, "Acc.py", Language::Java);
    assert!(
        java.contains("(double)") && java.contains("/ 100.0"),
        "Java would have answered 5 for 5.34, and said nothing.\n{java}"
    );
}

#[test]
fn a_target_whose_slash_is_already_a_float_division_is_left_alone() {
    let ts = translated(ACCOUNT, "acc.py", Language::TypeScript);
    assert!(
        ts.contains("this.cents / 100"),
        "TypeScript divides as the source did, with no repair.\n{ts}"
    );
    assert!(
        !ts.contains("as f64") && !ts.contains("(double)"),
        "and carries no coercion it does not need.\n{ts}"
    );
}

#[test]
fn floor_division_still_floors() {
    let source = "def half(n: int) -> int:\n    return n // 2\n";
    let rust = translated(source, "h.py", Language::Rust);
    assert!(
        !rust.contains("as f64"),
        "`//` asked for the truncating division and gets it.\n{rust}"
    );
}
