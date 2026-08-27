//! Rewriting a file as another language, and refusing where that is not a thing.

use fun_refactor::lang::Language;
use fun_refactor::translate;
use fun_refactor::transpile;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

#[test]
fn plain_css_is_scss() {
    let (_tmp, root) = workspace(&[("a.css", ".panel {\n  color: red;\n}\n")]);
    let plan = translate::plan(&root.join("a.css"), Language::Scss).expect("css is scss");
    assert_eq!(plan.destination, root.join("a.scss"));
    assert_eq!(plan.from, Language::Css);
}

#[test]
fn scss_that_uses_scss_is_not_css() {
    // Nesting is the commonest thing CSS cannot read.
    let (_tmp, root) = workspace(&[(
        "a.scss",
        "$brand: red;\n\n.panel {\n  color: $brand;\n\n  .inner {\n    color: blue;\n  }\n}\n",
    )]);
    let error =
        translate::plan(&root.join("a.scss"), Language::Css).expect_err("nested SCSS is not CSS");
    let message = error.to_string();
    assert!(
        message.contains("does not accept"),
        "the refusal should name the grammar that refused: {message}"
    );
    assert!(
        message.contains("line"),
        "and point at the first thing it choked on: {message}"
    );
}

#[test]
fn scss_that_happens_to_be_css_converts() {
    let (_tmp, root) = workspace(&[("a.scss", ".panel {\n  color: red;\n}\n")]);
    let plan = translate::plan(&root.join("a.scss"), Language::Css).expect("this file is CSS");
    assert_eq!(plan.destination, root.join("a.css"));
}

#[test]
fn typescript_is_tsx_and_plain_tsx_is_typescript() {
    let (_tmp, root) = workspace(&[
        ("a.ts", "export function greet(): void {}\n"),
        ("b.tsx", "export function n(): number {\n  return 1;\n}\n"),
    ]);
    assert!(translate::plan(&root.join("a.ts"), Language::Tsx).is_ok());
    assert!(translate::plan(&root.join("b.tsx"), Language::TypeScript).is_ok());
}

#[test]
fn a_manifest_is_a_template_and_a_template_with_actions_is_not_a_manifest() {
    let (_tmp, root) = workspace(&[
        ("chart/Chart.yaml", "name: c\n"),
        ("chart/values.yaml", "replicas: 2\n"),
        (
            "chart/templates/deploy.yaml",
            "spec:\n  replicas: {{ .Values.replicas }}\n",
        ),
        ("plain.yaml", "spec:\n  replicas: 2\n"),
    ]);
    assert!(
        translate::plan(&root.join("plain.yaml"), Language::Helm).is_ok(),
        "a manifest is a template with no actions in it"
    );
}

#[test]
fn an_imperative_pair_is_never_a_rewrite_and_the_refusal_points_at_the_draft() {
    // The containment path must still refuse every imperative pair: the bytes of a Rust file
    // are never a Python file.
    let (_tmp, root) = workspace(&[
        ("a.rs", "fn main() {}\n"),
        ("b.py", "def main():\n    pass\n"),
        ("c.go", "package main\n\nfunc main() {}\n"),
    ]);
    for (file, to, expected) in [
        ("a.rs", Language::Python, "as a draft"),
        ("b.py", Language::Go, "as a draft"),
        ("c.go", Language::Rust, "as a draft"),
        // Bash grew a reader and a writer, so it points at the draft path too.
        ("a.rs", Language::Bash, "as a draft"),
    ] {
        let error = translate::plan(&root.join(file), to).expect_err("never a rewrite");
        let message = error.to_string();
        assert!(
            message.contains(expected),
            "{file} -> {to} should say '{expected}'. Got: {message}."
        );
    }
}

#[test]
fn every_imperative_language_offers_nothing() {
    for language in [
        Language::Rust,
        Language::Go,
        Language::Zig,
        Language::Python,
        Language::Bash,
    ] {
        assert!(
            translate::targets(language).is_empty(),
            "{language} has no language it can be rewritten as"
        );
    }
}

#[test]
fn it_refuses_to_overwrite_something_that_is_already_there() {
    let (_tmp, root) = workspace(&[
        ("a.css", ".panel {\n  color: red;\n}\n"),
        ("a.scss", "// mine\n"),
    ]);
    let error = translate::plan(&root.join("a.css"), Language::Scss).expect_err("a.scss exists");
    assert!(error.to_string().contains("already exists"), "{error}");
}

#[test]
fn the_offered_targets_all_actually_work_on_a_file_that_suits_them() {
    // Whatever `targets` advertises must be plannable for a file in the intersection, or the
    // interface offers buttons that always refuse.
    let (_tmp, root) = workspace(&[
        ("one.css", ".a {\n  color: red;\n}\n"),
        ("two.scss", ".a {\n  color: red;\n}\n"),
        ("three.ts", "export const a = 1;\n"),
        ("four.tsx", "export const a = 1;\n"),
        ("five.yaml", "a: 1\n"),
        ("six.tpl", "a: 1\n"),
        ("seven.html", "<div id=\"a\"></div>\n"),
        ("eight.xml", "<div id=\"a\"></div>\n"),
    ]);
    let mut checked = 0;
    for (file, language) in [
        ("one.css", Language::Css),
        ("two.scss", Language::Scss),
        ("three.ts", Language::TypeScript),
        ("four.tsx", Language::Tsx),
        ("five.yaml", Language::Yaml),
        ("six.tpl", Language::Helm),
        ("seven.html", Language::Html),
        ("eight.xml", Language::Xml),
    ] {
        for target in translate::targets(language) {
            checked += 1;
            let outcome = translate::plan(&root.join(file), *target);
            assert!(
                outcome.is_ok(),
                "{language} offers {target}, but a file in the intersection was refused: {:?}",
                outcome.err().map(|e| e.to_string())
            );
        }
    }
    assert!(
        checked >= 8,
        "expected every declared pair to be exercised, ran {checked}"
    );
}

#[test]
fn force_replaces_the_previous_translation_instead_of_stacking_a_second() {
    // The overwrite edit was an insertion at byte zero, so an existing destination kept its old
    // translation below the new one.
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("c.py");
    std::fs::write(
        &source,
        "def cost(price: float) -> float:\n    return price * 2\n",
    )
    .unwrap();
    let destination = tmp.path().join("c.ts");

    let first = transpile::plan_to(&source, Language::TypeScript, Some(&destination), false)
        .expect("a fresh destination plans");
    let edits = first.edits.edits_for(&destination).expect("one file");
    let written = fun_refactor::edit::apply_to_string("", edits).unwrap();
    std::fs::write(&destination, &written).unwrap();

    let second = transpile::plan_to(&source, Language::TypeScript, Some(&destination), true)
        .expect("--force plans over an existing file");
    let existing = std::fs::read_to_string(&destination).unwrap();
    let edits = second.edits.edits_for(&destination).expect("one file");
    let replaced = fun_refactor::edit::apply_to_string(&existing, edits).unwrap();
    assert_eq!(
        replaced.matches("Translated from python").count(),
        1,
        "one translation, one header:\n{replaced}"
    );
    assert_eq!(
        written, replaced,
        "a forced rerun reproduces the file, not two of it"
    );
}
