//! A nested Java record crosses as the record it is, and statics as functions.
//!
//! `record Order(...)` inside a class was dropped as a comment while the header
//! counted one record carried; static methods gained a receiver their call sites
//! never pass. The record's accessor calls, `o.paid()`, become field reads where
//! the record crossed as fields, and a class emptied by the hoisting is only a
//! namespace and is not written at all.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

const ORDERS_JAVA: &str = "import java.util.List;\n\npublic class Orders {\n    \
    record Order(int id, String customer, double amount, boolean paid) {\n    }\n\n    \
    static double totalPaid(List<Order> orders) {\n        double sum = 0;\n        \
    for (Order o : orders) {\n            if (o.paid()) {\n                \
    sum = sum + o.amount();\n            }\n        }\n        return sum;\n    }\n}\n";

fn to_python(source: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("Orders.java");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("orders_out.txt");
    transpile::plan_to(&path, Language::Python, Some(&out), false)
        .expect("a plan")
        .output
}

#[test]
fn the_nested_record_is_a_dataclass_with_its_fields() {
    let out = to_python(ORDERS_JAVA);
    assert!(
        out.contains("class Order:") && out.contains("paid: bool"),
        "the record crosses with typed fields:\n{out}"
    );
    assert!(
        !out.contains("not translated: record_declaration"),
        "nothing about it is a gap any more:\n{out}"
    );
}

#[test]
fn the_static_method_is_a_module_function_without_self() {
    let out = to_python(ORDERS_JAVA);
    assert!(
        out.contains("def total_paid(orders:"),
        "no receiver its callers never pass:\n{out}"
    );
}

#[test]
fn accessor_calls_become_the_field_reads_they_are() {
    let out = to_python(ORDERS_JAVA);
    assert!(
        out.contains("if o.paid:") && out.contains("o.amount"),
        "the record crossed as fields, so the accessors read them:\n{out}"
    );
    assert!(
        !out.contains("o.paid()"),
        "no call to a field survives:\n{out}"
    );
}

#[test]
fn a_class_emptied_by_the_hoisting_is_not_written() {
    let out = to_python(ORDERS_JAVA);
    assert!(
        !out.contains("class Orders"),
        "the namespace shell says less than nothing:\n{out}"
    );
}
