//! What a member access is allowed to claim about its receiver.
//!
//! `Confidence::FieldBased` is defined as "matched by field/member name without knowing
//! the receiver's type — plausible but unproven", and `Exact` as "safe to edit". A
//! member access on a receiver of unknown type is the first of those by definition, and
//! the difference is not cosmetic: only the top two tiers are rewritten, so calling it
//! `Exact` means `fr rename` edits it without asking.

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

/// The receiver is from outside the workspace and has nothing to do with the class that
/// happens to declare a method of the same name. Claiming `exact` here rewrote a call on
/// a boto3 client to `client.sum()` because a `Cart` in the same file declared `total`.
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

/// The other side. A call through `self` or `this` has a receiver whose type is known —
/// it is the class the call is written inside — and lexical scope settles it before the
/// question of same-file uniqueness arises. Losing this would cost far more than the
/// overclaim it guards against.
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

/// And a plain call to a function defined in the same file is not a member access at
/// all, so it keeps the tier it earned.
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
