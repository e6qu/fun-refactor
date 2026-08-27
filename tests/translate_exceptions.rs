//! Exceptions cross under the target's own names, and their text stays their text.

use fun_refactor::lang::Language;
use fun_refactor::transpile;
use std::path::PathBuf;

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        std::fs::write(tmp.path().join(name), content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

#[test]
fn a_typescript_error_reaches_python_as_a_raise_it_can_catch() {
    let source = "function risky(n: number): number {\n    if (n < 0) {\n        \
                  throw new Error(\"negative: \" + n);\n    }\n    return n * 2;\n}\n\n\
                  function guarded(n: number): void {\n    try {\n        \
                  console.log(\"ok\", risky(n));\n    } catch (e) {\n        \
                  console.log(\"caught\", (e as Error).message);\n    }\n}\n";
    let (_tmp, root) = workspace(&[("exc.ts", source)]);
    let plan = transpile::plan(&root.join("exc.ts"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains("raise Exception(f\"negative: {n}\")"),
        "Error is the general Exception, and the number is said as text.\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("print(\"caught\", str(e))"),
        "the catch binding's `.message` is `str(e)` here.\n{}",
        plan.output
    );
}

#[test]
fn javas_exception_spellings_reach_python_as_pythons() {
    let source = "public class Checks {\n    static int positive(int n) {\n        \
                  if (n < 0) {\n            \
                  throw new IllegalArgumentException(\"negative: \" + n);\n        }\n        \
                  return n;\n    }\n\n    static void guarded(int n) {\n        \
                  try {\n            System.out.println(positive(n));\n        \
                  } catch (IllegalArgumentException e) {\n            \
                  System.out.println(\"caught \" + e.getMessage());\n        }\n    }\n}\n";
    let (_tmp, root) = workspace(&[("Checks.java", source)]);
    let plan = transpile::plan(&root.join("Checks.java"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains("raise ValueError(f\"negative: {n}\")"),
        "the argument complaint is ValueError's.\n{}",
        plan.output
    );
    // The canonical error model carries the message; the class dissolves into it, and the catch
    // takes whatever failure arrives.
    assert!(
        plan.output.contains("except Exception as e:"),
        "the catch takes the canonical failure.\n{}",
        plan.output
    );
    assert!(
        plan.output.contains("print(f\"caught {str(e)}\")"),
        "`e.getMessage()` is `str(e)` here:\n{}",
        plan.output
    );
}

#[test]
fn pythons_builtin_exceptions_reach_typescript_as_declared_classes() {
    let source = "def risky(n):\n    if n < 0:\n        \
                  raise ValueError(\"negative: \" + str(n))\n    return n * 2\n\n\n\
                  def guarded(n):\n    try:\n        print(\"ok\", risky(n))\n    \
                  except ValueError as e:\n        print(\"caught\", e)\n";
    let (_tmp, root) = workspace(&[("exc.py", source)]);
    let plan = transpile::plan(&root.join("exc.py"), Language::TypeScript).expect("a draft");
    // The throw keeps its declared class.
    for expected in [
        "class ValueError extends Error {}",
        "throw new ValueError(`negative: ${String(n)}`);",
        "console.log(\"caught\", (e as Error).message);",
    ] {
        assert!(
            plan.output.contains(expected),
            "missing `{expected}`:\n{}",
            plan.output
        );
    }
}

#[test]
fn the_general_exception_is_typescripts_own_error_and_declares_nothing() {
    let source = "def run():\n    raise Exception(\"boom\")\n";
    let (_tmp, root) = workspace(&[("boom.py", source)]);
    let plan = transpile::plan(&root.join("boom.py"), Language::TypeScript).expect("a draft");
    assert!(
        plan.output.contains("throw new Error(\"boom\");"),
        "Exception is the base Error here:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains("class Error"),
        "the language already has Error; no class is invented for it.\n{}",
        plan.output
    );
}
