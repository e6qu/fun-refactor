use fun_refactor::{lang::Language, parse::Parsers};
use std::{fs, process::Command};

#[test]
fn python_layout_matches_independent_compiler() {
    let cases = [
        (false, "# \\\n    x = 1\n"),
        (true, "def f():\n    # \\\n    x = 1\n    return x\n"),
        (false, "def f():\n    x = 1\n        return x\n"),
        (
            false,
            "def f():\n    if True:\n        x = 1\n      return x\n",
        ),
        (false, "  x = 1\n"),
        (false, "def f():\n\tx = 1\n        return x\n"),
        (false, "if True:\n    pass\n  else:\n    pass\n"),
        (
            true,
            "def f():\n    if True:\n        x = 1\n    return x\n",
        ),
        (true, "def f(): return 1; return 2\nx = 1; y = 2\n"),
        (true, "def f(\n    a,\n):\n    return (\n  a + 1\n    )\n"),
        (
            true,
            "def f():\n    # comment\n    x = '''text\nno indent\n'''\n    return x\n",
        ),
        (true, "@decorator\ndef f():\n\treturn 1\n"),
        (
            true,
            "def f():\n    x = 1; \\\n        y = 2\n    return x + y\n",
        ),
        (true, "if True:\n    pass\nelse:\n    pass\n"),
        (
            true,
            "try:\n    pass\nexcept ValueError:\n    pass\nfinally:\n    pass\n",
        ),
        (
            true,
            "match 1:\n    case 1:\n        pass\n    case _:\n        pass\n",
        ),
    ];
    for (valid, source) in cases {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("case.py");
        fs::write(&path, source).unwrap();
        let compiled = Command::new("python3")
            .args([
                "-B",
                "-c",
                "import ast,sys; ast.parse(open(sys.argv[1]).read())",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(compiled.status.success(), valid, "{source}\n{compiled:?}");
        let parsed = Parsers::new().parse(Language::Python, source).unwrap();
        assert_eq!(
            !parsed.has_errors(),
            valid,
            "{source}\n{}",
            parsed.root().to_sexp()
        );
        assert_eq!(parsed.error_spans().is_empty(), valid, "{source}");
    }
}

#[test]
fn author_refuses_unexpected_indent_without_writing_or_saving() {
    let temp = tempfile::tempdir().unwrap();
    let source = "def f():\n    return 1\n";
    fs::write(temp.path().join("app.py"), source).unwrap();
    let input = temp.path().join("bad.txt");
    fs::write(&input, "x = 1\n    return x\n").unwrap();
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_fr"))
            .arg("--json")
            .arg("-C")
            .arg(temp.path())
            .args(args)
            .output()
            .unwrap()
    };
    let found = run(&["project", "find", "f"]);
    assert!(found.status.success());
    let found: serde_json::Value = serde_json::from_slice(&found.stdout).unwrap();
    let handle = found["rows"][0][0].as_str().unwrap();
    let result = run(&[
        "author",
        "replace-body",
        handle,
        "--from",
        input.to_str().unwrap(),
        "--write",
    ]);
    assert!(!result.status.success(), "{result:?}");
    assert!(String::from_utf8_lossy(&result.stderr).contains("complete block"));
    assert_eq!(
        fs::read_to_string(temp.path().join("app.py")).unwrap(),
        source
    );
}
