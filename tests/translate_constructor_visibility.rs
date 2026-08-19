//! `__init__` is how Python spells a public constructor.
//!
//! Its underscores were read as Python's mark for "internal", so Java produced
//! a `private Account(...)` on a public class and Rust a private `fn new`.
//! Neither type could be built from outside the file that declared it, and
//! nothing said so.

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
    self.cents = cents\n\n    def _audit(self) -> int:\n        return self.cents\n";

#[test]
fn java_can_construct_the_class_it_declares() {
    let java = translated(ACCOUNT, "Account.py", Language::Java);
    assert!(
        java.contains("public Account("),
        "a private constructor makes the class unbuildable from outside.\n{java}"
    );
}

#[test]
fn rust_can_construct_the_struct_it_declares() {
    let rust = translated(ACCOUNT, "account.py", Language::Rust);
    assert!(
        rust.contains("pub fn new("),
        "a private `new` makes the struct unbuildable from outside.\n{rust}"
    );
}

#[test]
fn one_leading_underscore_still_means_internal() {
    let java = translated(ACCOUNT, "Account.py", Language::Java);
    assert!(
        !java.contains("public int audit") && !java.contains("public int _audit"),
        "`_audit` is Python saying keep out, and that carries.\n{java}"
    );
}
