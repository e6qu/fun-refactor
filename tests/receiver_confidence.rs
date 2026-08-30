//! What a member access is allowed to claim about its receiver.

use fun_refactor::index::Index;
use fun_refactor::model::Confidence;
use fun_refactor::scan::ScanOptions;

fn confidences(files: &[(&str, &str)], name: &str) -> Vec<Confidence> {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (file, content) in files {
        std::fs::write(tmp.path().join(file), content).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    index
        .references
        .iter()
        .filter(|r| r.name == name)
        .map(|r| r.confidence)
        .collect()
}

/// The receiver is from outside the workspace and has nothing to do with the class that happens
/// to declare a method of the same name.
#[test]
fn a_call_on_a_foreign_receiver_is_not_exact() {
    let found = confidences(
        &[(
            "a.py",
            "import boto3\n\n\nclass Cart:\n    def total(self):\n        return 1\n\n\n\
             def go(cart: Cart):\n    cart.total()\n    \
             client = boto3.client(\"s3\")\n    client.total()\n    return 0\n",
        )],
        "total",
    );
    assert_eq!(found.len(), 2, "both calls are references: {found:?}");
    for confidence in &found {
        assert!(
            !confidence.is_safe_to_rewrite(),
            "nothing here knows what the receiver is, so nothing may rewrite it: {found:?}"
        );
    }
}

/// The same shape in TypeScript: an instance of an imported class, and a class in this
/// file that declares a method of the same name.
#[test]
fn a_call_on_an_imported_instance_is_not_exact() {
    let found = confidences(
        &[(
            "b.ts",
            "import { S3 } from \"aws-sdk\";\n\n\
             export class Basket {\n  weigh(): number {\n    return 1;\n  }\n}\n\n\
             export function go(basket: Basket) {\n  basket.weigh();\n  \
             const client = new S3();\n  client.weigh();\n}\n",
        )],
        "weigh",
    );
    assert_eq!(found.len(), 2, "got {found:?}");
    for confidence in &found {
        assert!(!confidence.is_safe_to_rewrite(), "got {found:?}");
    }
}

/// The other side.
#[test]
fn a_call_through_self_is_still_exact() {
    let python = confidences(
        &[(
            "a.py",
            "class Cart:\n    def total(self):\n        return self.fee()\n\n    \
             def fee(self):\n        return 1\n",
        )],
        "fee",
    );
    assert_eq!(python, vec![Confidence::Exact], "python `self`");

    let typescript = confidences(
        &[(
            "b.ts",
            "export class Basket {\n  weigh(): number {\n    return this.tare();\n  }\n  \
             tare(): number {\n    return 1;\n  }\n}\n",
        )],
        "tare",
    );
    assert_eq!(typescript, vec![Confidence::Exact], "typescript `this`");
}

/// And a plain call to a function defined in the same file is not a member access at all.
#[test]
fn a_call_to_a_function_in_the_same_file_is_still_exact() {
    let found = confidences(
        &[(
            "a.py",
            "def helper():\n    return 1\n\n\ndef go():\n    return helper()\n",
        )],
        "helper",
    );
    assert_eq!(found, vec![Confidence::Exact]);
}

/// The same overclaim, one branch up.
#[test]
fn a_call_on_an_unknown_receiver_inside_the_declaring_class_is_not_exact() {
    let found = confidences(
        &[(
            "a.py",
            "import boto3\n\n\nclass Cart:\n    def total(self):\n        return 1\n\n    \
             def run(self, client):\n        return client.total()\n",
        )],
        "total",
    );
    assert_eq!(found.len(), 1, "got {found:?}");
    assert!(
        !found[0].is_safe_to_rewrite(),
        "`client` is a parameter of unknown type: {found:?}"
    );
}

/// The property itself, instead of an instance of it.
#[test]
fn no_route_through_the_resolver_makes_an_unknown_receiver_rewritable() {
    let shapes: &[(&str, &[(&str, &str)])] = &[
        (
            "unique in this file",
            &[(
                "a.py",
                "class Cart:\n    def total(self):\n        return 1\n\n\n\
                 def go(x):\n    return x.total()\n",
            )],
        ),
        (
            "enclosing scope",
            &[(
                "a.py",
                "class Cart:\n    def total(self):\n        return 1\n\n    \
                 def go(self, x):\n        return x.total()\n",
            )],
        ),
        (
            "declared in another file",
            &[
                (
                    "cart.py",
                    "class Cart:\n    def total(self):\n        return 1\n",
                ),
                ("go.py", "def go(x):\n    return x.total()\n"),
            ],
        ),
        (
            "a field, not a call",
            &[(
                "a.py",
                "class Cart:\n    def __init__(self):\n        self.total = 0\n\n\n\
                 def go(x):\n    return x.total\n",
            )],
        ),
    ];

    for (what, files) in shapes {
        for confidence in confidences(files, "total") {
            assert!(
                !confidence.is_safe_to_rewrite(),
                "{what}: an unknown receiver came back as {confidence:?}"
            );
        }
    }
}

/// "I could not say what this refers to.
#[test]
fn an_unresolved_reference_never_claims_a_rewritable_tier() {
    let files: &[(&str, &str)] = &[
        (
            "a.py",
            "import boto3\n\n\nclass Cart:\n    def total(self):\n        return 1\n\n\n\
             def go(x, cart: Cart):\n    cart.total()\n    x.total()\n    \
             return boto3.client(\"s3\").total()\n",
        ),
        (
            "b.ts",
            "import { S3 } from \"aws-sdk\";\n\nexport class Basket {\n  \
             weigh(): number {\n    return this.weigh();\n  }\n}\n\n\
             export function go(anything: unknown) {\n  new S3().weigh();\n  \
             (anything as Basket).weigh();\n}\n",
        ),
        (
            "c.go",
            "package main\n\nfunc Total() int {\n\treturn 1\n}\n\n\
             func run(v interface{ Total() int }) int {\n\treturn v.Total()\n}\n",
        ),
    ];
    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (file, content) in files {
        std::fs::write(tmp.path().join(file), content).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");

    let liars: Vec<_> = index
        .references
        .iter()
        .filter(|r| r.target.is_none() && r.confidence.is_safe_to_rewrite())
        .map(|r| format!("{} at {}", r.name, r.span.start))
        .collect();
    assert!(
        liars.is_empty(),
        "resolved to nothing, yet safe to rewrite: {liars:?}"
    );
}

/// Two classes each declare a `size`.
#[test]
fn a_typed_receiver_resolves_the_member_it_owns() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let java = "class A {\n    int size(int n) { return n; }\n}\n\
                class B {\n    int size(int n) { return n + 1; }\n}\n\
                class Use {\n    int go() {\n        B b = new B();\n        return b.size(2);\n    }\n}\n";
    std::fs::write(tmp.path().join("A.java"), java).expect("the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let call = index
        .references
        .iter()
        .find(|r| r.name == "size" && r.receiver.as_deref() == Some("b"))
        .expect("the call through `b`");
    let target = call.target.expect("a typed receiver names its member");
    let owner = index.symbol(target).and_then(|s| s.qualifier.clone());
    assert_eq!(
        owner.as_deref(),
        Some("B"),
        "the receiver's own class answers"
    );
    assert!(
        !call.confidence.is_safe_to_rewrite(),
        "known target, uncrossed rewrite line: {:?}",
        call.confidence
    );
}

/// The same call through a receiver nothing types stays unresolved: several members
/// share the name and no evidence picks one.
#[test]
fn an_untyped_receiver_still_resolves_no_shared_member() {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let java = "class A {\n    int size(int n) { return n; }\n}\n\
                class B {\n    int size(int n) { return n + 1; }\n}\n\
                class Use {\n    int go(Object anything) {\n        \
                return ((B) anything).size(2);\n    }\n}\n";
    std::fs::write(tmp.path().join("A.java"), java).expect("the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let call = index
        .references
        .iter()
        .find(|r| r.name == "size")
        .expect("the call");
    assert!(
        call.target.is_none(),
        "a cast is not a binding this reads, so nothing picks a member: {:?}",
        call.target
    );
}
