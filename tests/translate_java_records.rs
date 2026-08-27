//! A nested Java record crosses as the record it is, and statics as functions.

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
        "the record crosses with typed fields.\n{out}"
    );
    assert!(
        !out.contains("not translated: record_declaration"),
        "nothing about it is a gap any more.\n{out}"
    );
}

#[test]
fn the_static_method_is_a_module_function_without_self() {
    let out = to_python(ORDERS_JAVA);
    // The method is package-private, which Python spells with the leading underscore.
    assert!(
        out.contains("_total_paid(orders:"),
        "no receiver its callers never pass:\n{out}"
    );
    assert!(
        !out.contains("total_paid(self"),
        "and no invented one:\n{out}"
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

const FEATURES_JAVA: &str = "public class Features {\n    \
    record Person(String name, int age) implements Greeter {\n        \
    public String name() {\n            return name;\n        }\n    }\n\n    \
    record Tagged(String label) implements Greeter {\n        \
    public String label() {\n            return \"tag:\" + label;\n        }\n    }\n}\n";

#[test]
fn a_records_implements_clause_carries_as_its_base() {
    // `implements Greeter` was dropped without a word, so the record crossed as
    // a type with no relation to the interface its callers know it by.
    let out = to_python(FEATURES_JAVA);
    assert!(
        out.contains("class Person(Greeter):"),
        "the one interface rides in the base slot.\n{out}"
    );
}

#[test]
fn a_spelled_out_accessor_does_not_collide_with_its_field() {
    // `record Person(String name, ...)` with a compact `public String name()` declares the
    // accessor twice.
    let out = to_python(FEATURES_JAVA);
    assert!(out.contains("name: str"), "the field is there.\n{out}");
    assert!(
        !out.contains("def name("),
        "and no same-named method stands beside it.\n{out}"
    );
}

#[test]
fn an_overriding_accessor_body_is_said_beside_the_field_it_stood_for() {
    let out = to_python(FEATURES_JAVA);
    assert!(
        out.contains("overrode the record's `label()` accessor"),
        "a body that did more than return the field is not dropped in silence.\n{out}"
    );
}

#[test]
fn multiple_interfaces_are_said_in_prose_beside_the_type() {
    let source = "public class Multi {\n    \
        record Pair(int a) implements Comparable, Cloneable {\n    }\n}\n";
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("Multi.java");
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("multi_out.txt");
    let plan = transpile::plan_to(&path, Language::Python, Some(&out), false)
        .expect("a plan")
        .output;
    assert!(
        plan.contains("implements Comparable, Cloneable"),
        "one base slot exists, and the rest is prose instead of silence.\n{plan}"
    );
}
