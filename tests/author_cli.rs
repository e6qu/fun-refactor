use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run(root: &Path, args: &[&str]) -> (bool, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["--json", "--no-cache", "-C"])
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{args:?}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), value)
}

fn ok(root: &Path, args: &[&str]) -> Value {
    let (success, value) = run(root, args);
    assert!(success, "{args:?}: {value}");
    value
}

fn fixture(source: &str, body: &[u8]) -> (tempfile::TempDir, PathBuf, PathBuf) {
    fixture_file("app.rs", source, body)
}

fn fixture_file(name: &str, source: &str, body: &[u8]) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join(name), source).unwrap();
    let input = temp.path().join("body.txt");
    fs::write(&input, body).unwrap();
    (temp, root, input)
}

fn selection(root: &Path, name: &str) -> (String, String) {
    let map = ok(
        root,
        &[
            "project",
            "map",
            "--locals",
            "--depth",
            "64",
            "--fields",
            "handle,name",
        ],
    );
    let row = map["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[1] == name)
        .unwrap_or_else(|| panic!("missing {name}: {map}"));
    (
        row[0].as_str().unwrap().to_owned(),
        map["revision"].as_str().unwrap().to_owned(),
    )
}

fn replace(root: &Path, handle: &str, input: &Path, flags: &[&str]) -> (bool, Value) {
    let mut args = vec![
        "author",
        "replace-body",
        handle,
        "--from",
        input.to_str().unwrap(),
    ];
    args.extend(flags);
    run(root, &args)
}

#[test]
fn stale_context_basis_refuses_author_writes_and_saved_plans_before_persistence() {
    for intent in ["--write", "--save-plan"] {
        let original = "fn calc(n: i32) -> i32 { n + 1 }\n";
        let (_temp, root, input) = fixture(original, b"{ n * 2 }");
        let map = ok(&root, &["project", "map"]);
        let basis = map["context_basis"].as_str().unwrap();
        let (handle, _) = selection(&root, "calc");
        let changed = "fn calc(n: i32) -> i32 { n + 2 }\n";
        fs::write(root.join("app.rs"), changed).unwrap();

        let (success, error) = replace(&root, &handle, &input, &[intent, "--context-basis", basis]);
        assert!(!success, "{intent}: {error}");
        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("stale or conflicting context basis"));
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), changed);
        assert!(!root.join(".fr-history").exists());
    }
}

fn compiled_result(root: &Path) -> Vec<u8> {
    let output_path = root.parent().unwrap().join("compiled");
    let output = Command::new("rustc")
        .args(["--edition=2021", "-o"])
        .arg(&output_path)
        .arg(root.join("app.rs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(output_path).output().unwrap();
    assert!(output.status.success());
    output.stdout
}

#[test]
fn saves_reviewed_body_and_preserves_context_through_apply_undo_redo_and_patch() {
    let source = "// π stays outside.\r\n#[inline]\r\npub fn calc(n: i32) -> i32 { n + 1 }\r\nfn main() { println!(\"{}\", calc(3)); }\r\n";
    let (_temp, root, input) = fixture(source, b"\n{ n * 2 }\n");
    assert_eq!(compiled_result(&root), b"4\n");
    let (handle, _) = selection(&root, "calc");
    let (success, preview) = replace(&root, &handle, &input, &["--diff-bytes", "7"]);
    assert!(success, "{preview}");
    assert_eq!(preview["schema"], "fr-author-1");
    assert_eq!(preview["applied"], false);
    assert_eq!(preview["behavior_checked"], false);
    assert_eq!(preview["body"]["after_bytes"], 9);
    assert!(preview["diff"]["text"].as_str().unwrap().len() <= 7);
    assert!(preview["diff"]["omitted_bytes"].as_u64().unwrap() > 0);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    assert!(!root.join(".fr-history").exists());
    let (success, complete) = replace(&root, &handle, &input, &[]);
    assert!(success, "{complete}");
    let unicode = complete["diff"].as_str().unwrap().find('π').unwrap();
    let budget = (unicode + 1).to_string();
    let (success, clipped) = replace(&root, &handle, &input, &["--diff-bytes", &budget]);
    assert!(success, "{clipped}");
    assert_eq!(clipped["diff"]["text"].as_str().unwrap().len(), unicode);
    let (success, saved) = replace(&root, &handle, &input, &["--save-plan"]);
    assert!(success, "{saved}");
    let id = saved["transaction"].as_u64().unwrap().to_string();
    fs::write(&input, b"{ 999 }").unwrap();
    ok(&root, &["history", "apply", &id, "--write"]);
    let changed = source.replace("{ n + 1 }", "{ n * 2 }");
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), changed);
    assert_eq!(compiled_result(&root), b"6\n");
    assert!(!replace(&root, &handle, &input, &["--write"]).0);
    fs::write(root.join("unrelated.rs"), "fn untouched() {}\n").unwrap();
    ok(&root, &["history", "undo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    ok(&root, &["history", "patch", &id, "--check"]);
    let patch = ok(&root, &["history", "patch", &id]);
    assert!(patch["patch"].as_str().unwrap().contains("n * 2"));
    ok(&root, &["history", "redo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), changed);
    assert_eq!(
        fs::read_to_string(root.join("unrelated.rs")).unwrap(),
        "fn untouched() {}\n"
    );
    fs::write(root.join("app.rs"), changed + "// A later edit.\n").unwrap();
    assert!(!run(&root, &["history", "undo", &id, "--write"]).0);
}

#[test]
fn stale_revisions_include_other_sources_and_manifest_changes() {
    for fault in ["source", "manifest", "input-in-workspace"] {
        let (_temp, root, input) = fixture("fn calc() { }\n", b"{ let _x = 1; }");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"fixture\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        let (handle, _) = selection(&root, "calc");
        match fault {
            "source" => fs::write(root.join("other.rs"), "fn other() {}\n").unwrap(),
            "manifest" => fs::write(
                root.join("Cargo.toml"),
                "[package]\nname=\"changed\"\nversion=\"0.1.0\"\n",
            )
            .unwrap(),
            _ => fs::write(root.join("replacement.rs"), "fn extra() {}\n").unwrap(),
        }
        let (success, report) = replace(&root, &handle, &input, &["--write"]);
        assert!(!success, "{fault}: {report}");
        assert!(report["error"]["message"]
            .as_str()
            .unwrap()
            .contains("stale"));
        assert_eq!(
            fs::read_to_string(root.join("app.rs")).unwrap(),
            "fn calc() { }\n"
        );
        assert!(!root.join(".fr-history").exists());
    }
}

#[test]
fn rejects_escaped_blocks_invalid_bytes_and_oversized_inputs_before_recording() {
    for body in [
        b"".to_vec(),
        b"{ let x = ; }".to_vec(),
        b"{} fn escape() {}".to_vec(),
        b"{} // trailing".to_vec(),
        b"unsafe {}".to_vec(),
        b"{\0}".to_vec(),
        vec![255],
        vec![b' '; 65537],
    ] {
        let (_temp, root, input) = fixture("fn calc() {}\n", &body);
        let (handle, _) = selection(&root, "calc");
        let (success, report) = replace(&root, &handle, &input, &["--save-plan"]);
        assert!(!success, "{report}");
        assert_eq!(
            fs::read_to_string(root.join("app.rs")).unwrap(),
            "fn calc() {}\n"
        );
        assert!(!root.join(".fr-history").exists());
    }
}

#[test]
fn refuses_nonfunctions_missing_bodies_and_original_syntax_errors() {
    for source in [
        "const calc: i32 = 1;\n",
        "fn outer() { let calc = 1; }\n",
        "fn outer() { let calc = || { 1 }; }\n",
        "trait T { fn calc(&self); }\n",
        "fn calc() { let broken = ; }\n",
    ] {
        let (_temp, root, input) = fixture(source, b"{}");
        let (handle, _) = selection(&root, "calc");
        assert!(!replace(&root, &handle, &input, &["--write"]).0, "{source}");
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    }
    let (_temp, root, input) = fixture("fn main() {}\n", b"{}");
    fs::write(root.join("other.py"), "def calc():\n    pass\n").unwrap();
    let (handle, _) = selection(&root, "calc");
    let (success, report) = replace(&root, &handle, &input, &["--write"]);
    assert!(!success);
    assert!(report["error"]["message"]
        .as_str()
        .unwrap()
        .contains("Rust"));
    let (file, _) = selection(&root, "app.rs");
    assert!(!replace(&root, &file, &input, &["--write"]).0);
}

#[test]
fn methods_default_trait_bodies_nested_functions_and_generic_headers_keep_surrounding_bytes() {
    for source in [
        "struct S; impl S { pub fn calc(&self) -> i32 { 1 } }\n",
        "trait T { fn calc(&self) -> i32 { 1 } }\n",
        "fn outer() -> i32 { fn calc() -> i32 { 1 } calc() }\n",
        "#[inline]\npub async unsafe fn calc<T: Copy>(n: T) -> i32 where T: Send { 1 }\n",
    ] {
        let (_temp, root, input) = fixture(source, b"{ 2 }");
        let (handle, _) = selection(&root, "calc");
        let (success, report) = replace(&root, &handle, &input, &["--write"]);
        assert!(success, "{source}: {report}");
        assert_eq!(report["applied"], true);
        assert_eq!(
            fs::read_to_string(root.join("app.rs")).unwrap(),
            source.replace("{ 1 }", "{ 2 }")
        );
    }
}

#[test]
fn short_ids_require_revision_and_equal_bodies_do_not_create_history() {
    let (_temp, root, input) = fixture("fn calc() {}\n", b" \n{}\n ");
    let (handle, revision) = selection(&root, "calc");
    let short = handle.rsplit(':').next().unwrap();
    assert!(!replace(&root, short, &input, &["--write"]).0);
    let (success, report) = replace(&root, short, &input, &["--revision", &revision, "--write"]);
    assert!(success, "{report}");
    assert_eq!(report["changed"], false);
    assert_eq!(report["applied"], false);
    assert!(report["transaction"].is_null());
    assert!(!root.join(".fr-history").exists());
    assert!(!replace(&root, &handle, &input, &["--save-plan", "--write"]).0);
    assert!(!replace(&root, &handle, &input, &["--diff-bytes", "65537"]).0);
}

#[test]
fn supports_exact_body_size_limit_and_reports_omitted_diff_bytes() {
    let body = format!("{{{}}}", " ".repeat(65534));
    let (_temp, root, input) = fixture("fn calc() {}\n", body.as_bytes());
    let (handle, _) = selection(&root, "calc");
    let (success, report) = replace(&root, &handle, &input, &["--diff-bytes", "0", "--write"]);
    assert!(success, "{report}");
    assert_eq!(report["body"]["after_bytes"], 65536);
    assert_eq!(report["diff"]["text"], "");
    let source = format!("fn calc() {{{}}}\n", " ".repeat(65535));
    fs::write(root.join("app.rs"), &source).unwrap();
    fs::write(&input, b"{}").unwrap();
    let (handle, _) = selection(&root, "calc");
    assert!(!replace(&root, &handle, &input, &["--write"]).0);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
}

#[cfg(unix)]
#[test]
fn refuses_input_symlinks_and_retains_target_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (temp, root, input) = fixture("fn calc() {}\n", b"{ let _x = 1; }");
    fs::set_permissions(root.join("app.rs"), fs::Permissions::from_mode(0o640)).unwrap();
    let linked = temp.path().join("linked");
    symlink(&input, &linked).unwrap();
    let (handle, _) = selection(&root, "calc");
    assert!(!replace(&root, &handle, &linked, &["--write"]).0);
    assert!(replace(&root, &handle, &input, &["--write"]).0);
    assert_eq!(
        fs::metadata(root.join("app.rs"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o640
    );
}

#[test]
fn typescript_and_tsx_declarations_and_methods_preserve_their_headers() {
    for extension in ["ts", "tsx"] {
        for (name, source, body) in [
            ("calc", "// π\r\nexport default function calc<T>(n: T): number { return 1; }\r\n", "{ return 2; }"),
            ("calc", "export async function calc<T>(n: T): Promise<number> { return 1; }\n", "{ return 2; }"),
            ("calc", "export function* calc(): Generator<number> { yield 1; }\n", "{ yield 2; }"),
            ("calc", "class C { protected static async calc(n: number): Promise<number> { return 1; } }\n", "{ return 2; }"),
            ("calc", "class C { *calc(): Generator<number> { yield 1; } }\n", "{ yield 2; }"),
            ("calc", "class C { get calc(): number { return 1; } }\n", "{ return 2; }"),
            ("calc", "class C { set calc(n: number) { void n; } }\n", "{ void (n * 2); }"),
            ("#calc", "class C { #calc(): number { return 1; } }\n", "{ return 2; }"),
            ("constructor", "class C { constructor(public n: number) { void n; } }\n", "{ void (n * 2); }"),
            ("calc", "const obj = { calc(n: number): number { return 1; } };\n", "{ return 2; }"),
            ("calc", "function outer() { function calc(): number { return 1; } return calc(); }\n", "{ return 2; }"),
        ] {
            let file = format!("app.{extension}");
            let old = if body.contains("yield") { "{ yield 1; }" } else if body.contains("void") { "{ void n; }" } else { "{ return 1; }" };
            let (_temp, root, input) = fixture_file(&file, source, body.as_bytes());
            let (handle, _) = selection(&root, name);
            let (success, report) = replace(&root, &handle, &input, &["--write"]);
            assert!(success, "{file}: {source}: {report}");
            assert_eq!(report["applied"], true);
            assert_eq!(fs::read_to_string(root.join(&file)).unwrap(), source.replace(old, body));
        }
    }
    for extension in ["js", "jsx", "mts", "cts", "mjs", "cjs"] {
        let file = format!("app.{extension}");
        let (_temp, root, input) =
            fixture_file(&file, "function calc() { return 1; }\n", b"{ return 2; }");
        let (handle, _) = selection(&root, "calc");
        assert!(replace(&root, &handle, &input, &["--write"]).0);
        assert_eq!(
            fs::read_to_string(root.join(file)).unwrap(),
            "function calc() { return 2; }\n"
        );
    }
}

#[test]
fn typescript_refuses_unsupported_handles_and_never_edits_the_enclosing_function() {
    for extension in ["ts", "tsx"] {
        for source in [
            "const calc = () => 1;\n",
            "const calc = () => ({ value: 1 });\n",
            "const calc = ((() => 1) as () => number);\n",
            "const calc = ((() => ({ value: 1 })) satisfies () => object);\n",
            "const calc = ((wrap(() => { return 1; })) as () => number);\n",
            "const calc = (condition ? () => { return 1; } : () => { return 2; })!;\n",
            "const calc = (sideEffect(), () => { return 1; });\n",
            "const calc = (() => { return () => { return 1; }; })();\n",
            "const calc = ((candidate) satisfies { helper: () => number });\n",
            "const calc = wrap(() => { return 1; });\n",
            "const calc = condition ? () => { return 1; } : () => { return 2; };\n",
            "const { calc } = { calc: () => { return 1; } };\n",
            "class C { calc = () => 1; }\n",
            "class C { calc = (wrap(() => { return 1; }))!; }\n",
            "function outer() { const calc = wrap(() => { return 1; }); return calc(); }\n",
            "const outer = () => { let calc = 1; return calc; };\n",
            "const outer = function () { let calc = 1; return calc; };\n",
            "const outer = (calc: number) => { return calc; };\n",
            "class C { outer = (calc: number) => { return calc; }; }\n",
            "function outer() { let calc = 1; return calc; }\n",
            "class C { outer() { const calc = 1; return calc; } }\n",
            "abstract class C { abstract calc(): number; }\n",
            "interface C { calc(): number; }\n",
            "declare function calc(): number;\n",
            "function calc() { const broken = ; }\n",
        ] {
            let file = format!("app.{extension}");
            let (_temp, root, input) = fixture_file(&file, source, b"{ return 2; }");
            let (handle, _) = selection(&root, "calc");
            let (success, report) = replace(&root, &handle, &input, &["--save-plan"]);
            assert!(!success, "{source}: {report}");
            assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
            assert!(!root.join(".fr-history").exists());
        }
    }
}

#[test]
fn typescript_and_tsx_replacements_require_one_complete_block_in_the_target_grammar() {
    for extension in ["ts", "tsx"] {
        for body in [
            "",
            "{ const x = ; }",
            "{} function escaped() {}",
            "{};",
            "{} // trailing",
            "{} /* trailing */",
            "/* leading */ {}",
            "({ value: 2 })",
            "{ return <span>; }",
            "{ let value: = 2; }",
        ] {
            let file = format!("app.{extension}");
            let source = "function calc() { return 1; }\n";
            let (_temp, root, input) = fixture_file(&file, source, body.as_bytes());
            let (handle, _) = selection(&root, "calc");
            let (success, report) = replace(&root, &handle, &input, &["--write"]);
            assert!(!success, "{file}: {body}: {report}");
            assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
            assert!(!root.join(".fr-history").exists());
        }
    }
    let (_temp, root, input) =
        fixture_file("app.ts", "function calc() {}\n", b"{ return <span />; }");
    let (handle, _) = selection(&root, "calc");
    assert!(!replace(&root, &handle, &input, &["--write"]).0);
}

fn typescript_result(root: &Path, file: &str) -> Vec<u8> {
    let output_dir = root.parent().unwrap().join("compiled");
    let pinned = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/typesafety/typescript/node_modules/typescript/bin/tsc");
    let mut compiler = if pinned.is_file() {
        let mut node = Command::new("node");
        node.arg(pinned);
        node
    } else {
        Command::new("tsc")
    };
    let output = compiler
        .args([
            "--strict",
            "--target",
            "es2022",
            "--module",
            "commonjs",
            "--jsx",
            "react",
            "--jsxFactory",
            "h",
            "--outDir",
        ])
        .arg(&output_dir)
        .arg(root.join(file))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new("node")
        .arg(output_dir.join("app.js"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn typed_and_jsx_bodies_compile_and_run_through_saved_history() {
    for (file, source, old, new) in [
        ("app.ts", "export function calc(value: number): number { return value + 1; }\nconsole.log(calc(3));\n", "{ return value + 1; }", "{ return value * 2; }"),
        ("app.tsx", concat!("declare namespace JSX { type Element = string; interface IntrinsicElements { span: {}; } }\n\n", "function h(tag: string, props: unknown, child: unknown): string { return String(child); }\n", "function calc(value: number): JSX.Element { return <span>{value + 1}</span>; }\nconsole.log(calc(3));\n"), "{ return <span>{value + 1}</span>; }", "{ return <span>{value * 2}</span>; }"),
    ] {
        let (_temp, root, input) = fixture_file(file, source, new.as_bytes());
        assert_eq!(typescript_result(&root, file), b"4\n");
        let (handle, _) = selection(&root, "calc");
        let (success, saved) = replace(&root, &handle, &input, &["--save-plan"]);
        assert!(success, "{saved}");
        assert_eq!(saved["behavior_checked"], false);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
        let id = saved["transaction"].as_u64().unwrap().to_string();
        fs::write(&input, b"{ return 999; }").unwrap();
        ok(&root, &["history", "apply", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source.replace(old, new));
        assert_eq!(typescript_result(&root, file), b"6\n");
        assert!(!replace(&root, &handle, &input, &["--write"]).0);
        ok(&root, &["history", "undo", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
        ok(&root, &["history", "patch", &id, "--check"]);
        assert!(ok(&root, &["history", "patch", &id])["patch"].as_str().unwrap().contains("value * 2"));
        ok(&root, &["history", "redo", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source.replace(old, new));
        fs::write(root.join(file), source).unwrap();
        assert!(!run(&root, &["history", "undo", &id, "--write"]).0);
    }
}

#[test]
fn brace_spans_preserve_comments_and_semicolons_outside_the_body() {
    for (file, declaration, old, new) in [
        (
            "app.rs",
            "fn calc() -> i32 /* Before. */ { 1 }",
            "{ 1 }",
            "{ /* } */ 2 }",
        ),
        (
            "app.ts",
            "function calc(): number /* Before. */ { return 1; }",
            "{ return 1; }",
            "{ /* } */ return 2; }",
        ),
        (
            "app.tsx",
            "function calc(): number /* Before. */ { return 1; }",
            "{ return 1; }",
            "{ /* } */ return 2; }",
        ),
    ] {
        for tail in [
            " // π outside\r\n",
            " /* outside } */\n",
            "\n// outside\n",
            "\n",
        ] {
            let source = format!("{declaration}{tail}");
            let (_temp, root, input) = fixture_file(file, &source, new.as_bytes());
            let (handle, _) = selection(&root, "calc");
            let (success, report) = replace(&root, &handle, &input, &["--write"]);
            assert!(success, "{source}: {report}");
            assert_eq!(report["body"]["before_bytes"], old.len());
            assert_eq!(
                fs::read_to_string(root.join(file)).unwrap(),
                source.replace(old, new)
            );
        }
    }
    let source = "function calc() { return 1; }; // outside\n";
    let (_temp, root, input) = fixture_file("app.ts", source, b"{ return 2; }");
    let (handle, _) = selection(&root, "calc");
    let (success, report) = replace(&root, &handle, &input, &["--write"]);
    assert!(success, "{report}");
    assert_eq!(
        fs::read_to_string(root.join("app.ts")).unwrap(),
        source.replace("return 1", "return 2")
    );
}

#[test]
fn duplicate_names_select_only_the_specific_implementation_body() {
    for (source, new, expected_bodies) in [
        ("function calc(n: number): number;\nfunction calc(n: string): string;\nfunction calc(n: number | string): number | string { return n; }\n", "{ return n; }", vec!["{ return n; }"]),
        ("class C { get calc(): number { return 1; } set calc(n: number) { void n; } }\n", "{}", vec!["{ return 1; }", "{ void n; }"]),
    ] {
        let (_temp, root, input) = fixture_file("app.ts", source, new.as_bytes());
        let map = ok(&root, &["project", "map", "--depth", "64", "--fields", "handle,name"]);
        let mut bodies = Vec::new();
        for row in map["rows"].as_array().unwrap().iter().filter(|row| row[1] == "calc") {
            let (success, report) = replace(&root, row[0].as_str().unwrap(), &input, &[]);
            if success {
                let start = report["body"]["before_span"]["start"].as_u64().unwrap() as usize;
                let end = report["body"]["before_span"]["end"].as_u64().unwrap() as usize;
                bodies.push(&source[start..end]);
            }
        }
        assert_eq!(bodies, expected_bodies);
        assert_eq!(fs::read_to_string(root.join("app.ts")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
}

#[test]
fn direct_function_bindings_preserve_initializers_and_neighboring_declarations() {
    for extension in ["ts", "tsx"] {
        for (name, source) in [
            ("calc", "// π\r\nexport const calc = (n: number): number => /* Before. */ { return 1; }; // Outside.\r\n"),
            ("calc", "export let calc: (n: number) => number = n => { return 1; };\n"),
            ("calc", "var calc = function (n: number): number { return 1; };\n"),
            ("calc", "export const calc = function named<T>(n: T): number { return 1; };\n"),
            ("calc", "const calc = async <T,>(n: T): Promise<number> => { return 1; };\n"),
            ("calc", "const calc = async function (n: number): Promise<number> { return 1; };\n"),
            ("calc", "const before = () => { return 0; }, calc = () => { return 1; }, after = () => { return 3; };\n"),
            ("calc", "function outer() { const calc = () => { return 1; }; return calc(); }\n"),
            ("calc", "const outer = () => { const calc = () => { return 1; }; return calc(); };\n"),
            ("calc", "class C { public readonly calc = (n: number): number => { return 1; }; }\n"),
            ("calc", "class C { static calc: () => number = function named() { return 1; }; }\n"),
            ("#calc", "class C { #calc = () => { return 1; }; }\n"),
        ] {
            let file = format!("app.{extension}");
            let (_temp, root, input) = fixture_file(&file, source, b"{ return 2; }");
            let (handle, _) = selection(&root, name);
            let (success, report) = replace(&root, &handle, &input, &["--write"]);
            assert!(success, "{source}: {report}");
            assert_eq!(report["applied"], true);
            assert!(!report["signature"]["text"].as_str().unwrap().contains("return 1"));
            assert!(!report["signature"]["text"].as_str().unwrap().contains("return 0"));
            assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source.replace("{ return 1; }", "{ return 2; }"));
        }
        for source in [
            "const calc = function* named(): Generator<number> { yield 1; };\n",
            "export const calc = async function* (): AsyncGenerator<number> { yield 1; };\n",
            "class C { calc = function* () { yield 1; }; }\n",
        ] {
            let file = format!("app.{extension}");
            let (_temp, root, input) = fixture_file(&file, source, b"{ yield 2; }");
            let (handle, _) = selection(&root, "calc");
            let (success, report) = replace(&root, &handle, &input, &["--write"]);
            assert!(success, "{source}: {report}");
            assert_eq!(
                fs::read_to_string(root.join(file)).unwrap(),
                source.replace("{ yield 1; }", "{ yield 2; }")
            );
        }
    }
}

#[test]
fn shadowed_function_bindings_select_one_body_by_handle() {
    for source in [
        "const calc = () => { return 1; };\nfunction outer() { const calc = function () { return 1; }; return calc(); }\n",
        "const calc = (() => { return 1; }) satisfies () => number;\nfunction outer() { const calc = ((function () { return 1; }) as () => number)!; return calc(); }\n",
    ] {
        for selected in 0..2 {
            let (_temp, root, input) = fixture_file("app.ts", source, b"{ return 2; }");
            let map = ok(
                &root,
                &[
                    "project",
                    "map",
                    "--locals",
                    "--depth",
                    "64",
                    "--fields",
                    "handle,name",
                ],
            );
            let rows: Vec<_> = map["rows"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row[1] == "calc")
                .collect();
            assert_eq!(rows.len(), 2);
            let (success, report) = replace(
                &root,
                rows[selected][0].as_str().unwrap(),
                &input,
                &["--write"],
            );
            assert!(success, "{report}");
            let start = source
                .match_indices("{ return 1; }")
                .nth(selected)
                .unwrap()
                .0;
            let mut expected = source.to_owned();
            expected.replace_range(start..start + "{ return 1; }".len(), "{ return 2; }");
            assert_eq!(fs::read_to_string(root.join("app.ts")).unwrap(), expected);
        }
    }
}

#[test]
fn expression_body_changes_keep_lexical_receivers_recursion_and_jsx_through_history() {
    for (file, source, old, new) in [
        ("app.ts", "class C { base = 3; calc = (): number => { return this.base + 1; }; }\nconst detached = new C().calc; console.log(detached());\n", "{ return this.base + 1; }", "{ return this.base * 2; }"),
        ("app.ts", "const calc = function self(n: number): number { return n === 0 ? 1 : self(n - 1) + 1; };\nconsole.log(calc(3));\n", "{ return n === 0 ? 1 : self(n - 1) + 1; }", "{ return n === 0 ? 0 : self(n - 1) + 2; }"),
        ("app.ts", "const calc = function* (n: number): Generator<number> { yield n + 1; };\nconsole.log(calc(3).next().value);\n", "{ yield n + 1; }", "{ yield n * 2; }"),
        ("app.tsx", concat!("declare namespace JSX { type Element = string; interface IntrinsicElements { span: {}; } }\n\n", "function h(tag: string, props: unknown, child: unknown): string { return String(child); }\n", "const calc = (value: number): JSX.Element => { return <span>{value + 1}</span>; };\nconsole.log(calc(3));\n"), "{ return <span>{value + 1}</span>; }", "{ return <span>{value * 2}</span>; }"),
    ] {
        let (_temp, root, input) = fixture_file(file, source, new.as_bytes());
        assert_eq!(typescript_result(&root, file), b"4\n");
        let (handle, _) = selection(&root, "calc");
        let (success, preview) = replace(&root, &handle, &input, &["--diff-bytes", "0"]);
        assert!(success, "{preview}");
        assert_eq!(preview["diff"]["text"], "");
        assert!(!root.join(".fr-history").exists());
        let (success, saved) = replace(&root, &handle, &input, &["--save-plan"]);
        assert!(success, "{saved}");
        let id = saved["transaction"].as_u64().unwrap().to_string();
        fs::write(&input, b"{}").unwrap();
        ok(&root, &["history", "apply", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source.replace(old, new));
        assert_eq!(typescript_result(&root, file), b"6\n");
        assert!(!replace(&root, &handle, &input, &["--write"]).0);
        ok(&root, &["history", "undo", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
        ok(&root, &["history", "patch", &id, "--check"]);
        let patch = ok(&root, &["history", "patch", &id]);
        assert!(patch["patch"].as_str().unwrap().contains(new));
        ok(&root, &["history", "redo", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source.replace(old, new));
    }
}

fn declaration(root: &Path, handle: &str, input: &Path, flags: &[&str]) -> (bool, Value) {
    let mut args = vec![
        "author",
        "replace-declaration",
        handle,
        "--from",
        input.to_str().unwrap(),
    ];
    args.extend(flags);
    run(root, &args)
}

#[test]
fn declaration_changes_signature_and_behavior_through_saved_history() {
    let old = "fn calc(n: i32) -> i32 { n + 1 }";
    let new = "pub fn calc(n: i64) -> i64 { n * 2 }";
    let source = format!("// π\r\n/// Keep this documentation.\r\n#[inline]\r\n{old}\r\nfn main() {{ println!(\"{{}}\", calc(3)); }}\r\n");
    let (_temp, root, input) = fixture(&source, new.as_bytes());
    assert_eq!(compiled_result(&root), b"4\n");
    let (handle, _) = selection(&root, "calc");
    let (success, preview) = declaration(&root, &handle, &input, &["--diff-bytes", "0"]);
    assert!(success, "{preview}");
    assert_eq!(preview["query"], "replace-declaration");
    assert_eq!(preview["schema"], "fr-author-1");
    assert_eq!(preview["behavior_checked"], false);
    assert_eq!(preview["declaration"]["before_bytes"], old.len());
    assert_eq!(
        preview["replacement_signature"]["text"],
        "pub fn calc(n: i64) -> i64"
    );
    assert_eq!(preview["diff"]["text"], "");
    assert!(!root.join(".fr-history").exists());
    let (success, saved) = declaration(&root, &handle, &input, &["--save-plan"]);
    assert!(success, "{saved}");
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    let id = saved["transaction"].as_u64().unwrap().to_string();
    fs::write(&input, b"fn calc() {}").unwrap();
    ok(&root, &["history", "apply", &id, "--write"]);
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        source.replace(old, new)
    );
    assert_eq!(compiled_result(&root), b"6\n");
    assert!(!declaration(&root, &handle, &input, &["--write"]).0);
    fs::write(root.join("other.rs"), "fn untouched() {}\n").unwrap();
    ok(&root, &["history", "undo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    ok(&root, &["history", "patch", &id, "--check"]);
    assert!(ok(&root, &["history", "patch", &id])["patch"]
        .as_str()
        .unwrap()
        .contains(new));
    ok(&root, &["history", "redo", &id, "--write"]);
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        source.replace(old, new)
    );
    assert_eq!(
        fs::read_to_string(root.join("other.rs")).unwrap(),
        "fn untouched() {}\n"
    );
    fs::write(
        root.join("app.rs"),
        source.replace(old, new) + "// Later change.\n",
    )
    .unwrap();
    assert!(!run(&root, &["history", "undo", &id, "--write"]).0);
}

#[test]
fn declaration_replacements_refuse_renames_attributes_escaped_items_and_invalid_fragments() {
    for fragment in [
        "",
        "fn other() {}",
        "fn calc();",
        "/// Documentation without a function.",
        "//! Inner documentation.\nfn calc() {}",
        "/*! Inner documentation. */ fn calc() {}",
        "// Ordinary comment.\nfn calc() {}",
        "/// Documentation.\n// Ordinary comment.\nfn calc() {}",
        "#[doc = \"Documentation.\"] fn calc() {}",
        "/// Documentation.\n#[inline] fn calc() {}",
        "fn calc() { let x = ; }",
        "struct calc;",
        "fn calc() {} fn escaped() {}",
        "fn calc() {} // Trailing.",
        "// Leading.\nfn calc() {}",
        "#[inline]\nfn calc() {}",
        "#![allow(dead_code)]\nfn calc() {}",
        "/// Replacement documentation.\nfn calc() {}",
        "fn calc() {\0}",
    ] {
        let source = "#[inline]\nfn calc() {}\n";
        let (_temp, root, input) = fixture(source, fragment.as_bytes());
        let (handle, _) = selection(&root, "calc");
        let (success, report) = declaration(&root, &handle, &input, &["--save-plan"]);
        assert!(!success, "{fragment}: {report}");
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
}

#[test]
fn declaration_selection_refuses_nonfunctions_bodyless_items_and_broken_sources() {
    for (file, source) in [
        ("app.rs", "const calc: i32 = 1;\n"),
        ("app.rs", "fn outer() { let calc = 1; }\n"),
        ("app.rs", "trait T { fn calc(&self); }\n"),
        ("app.rs", "fn calc() { let x = ; }\n"),
        ("app.ts", "function calc() {}\n"),
    ] {
        let (_temp, root, input) = fixture_file(file, source, b"fn calc() {}");
        let (handle, _) = selection(&root, "calc");
        assert!(!declaration(&root, &handle, &input, &["--write"]).0);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
    let (_temp, root, input) = fixture("fn calc() {}\n", b"fn calc() {}");
    let (handle, _) = selection(&root, "app.rs");
    assert!(!declaration(&root, &handle, &input, &["--write"]).0);
}

#[test]
fn declaration_replacement_keeps_attributes_and_container_boundaries() {
    for (source, old, new) in [
        (
            "#[inline]\npub async unsafe fn calc<T: Copy>(n: T) -> i32 where T: Send { 1 }\n",
            "pub async unsafe fn calc<T: Copy>(n: T) -> i32 where T: Send { 1 }",
            "pub async unsafe fn calc<T: Clone>(n: T) -> i64 where T: Sync { 2 }",
        ),
        (
            "struct S; impl S { #[inline] pub fn calc(&self) -> i32 { 1 } }\n",
            "pub fn calc(&self) -> i32 { 1 }",
            "pub fn calc(&self) -> i64 { 2 }",
        ),
        (
            "trait T { #[inline] fn calc(&self) -> i32 { 1 } }\n",
            "fn calc(&self) -> i32 { 1 }",
            "fn calc(&self) -> i64 { 2 }",
        ),
        (
            "fn outer() { fn calc() -> i32 { 1 } }\n",
            "fn calc() -> i32 { 1 }",
            "fn calc() -> i64 { 2 }",
        ),
        (
            "fn r#calc() -> i32 { 1 }\n",
            "fn r#calc() -> i32 { 1 }",
            "fn r#calc() -> i64 { 2 }",
        ),
    ] {
        let (_temp, root, input) = fixture(source, new.as_bytes());
        let name = if source.contains("r#calc") {
            "r#calc"
        } else {
            "calc"
        };
        let (handle, _) = selection(&root, name);
        let (success, report) = declaration(&root, &handle, &input, &["--write"]);
        assert!(success, "{source}: {report}");
        assert_eq!(
            fs::read_to_string(root.join("app.rs")).unwrap(),
            source.replace(old, new)
        );
    }
}

#[test]
fn declaration_plans_enforce_revisions_flags_and_noop_writes() {
    let source = "fn calc() {}\n";
    let (_temp, root, input) = fixture(source, b" \nfn calc() {}\n ");
    let (handle, revision) = selection(&root, "calc");
    let short = handle.rsplit(':').next().unwrap();
    assert!(!declaration(&root, short, &input, &["--write"]).0);
    let (success, noop) = declaration(&root, short, &input, &["--revision", &revision, "--write"]);
    assert!(success, "{noop}");
    assert_eq!(noop["changed"], false);
    assert_eq!(noop["applied"], false);
    assert!(noop["transaction"].is_null());
    assert!(!root.join(".fr-history").exists());
    assert!(!declaration(&root, &handle, &input, &["--diff-bytes", "65537"]).0);
    assert!(!declaration(&root, &handle, &input, &["--write", "--save-plan"]).0);
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"changed\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    assert!(!declaration(&root, &handle, &input, &["--write"]).0);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
}

#[test]
fn declaration_size_limits_cover_the_complete_function() {
    let header = "fn calc() ";
    let exact = format!("{header}{{{}}}", " ".repeat(65536 - header.len() - 2));
    let (_temp, root, input) = fixture("fn calc() {}\n", exact.as_bytes());
    let (handle, _) = selection(&root, "calc");
    let (success, report) = declaration(&root, &handle, &input, &["--diff-bytes", "0", "--write"]);
    assert!(success, "{report}");
    assert_eq!(report["declaration"]["after_bytes"], 65536);
    let oversized = exact.replacen('{', "{ ", 1);
    fs::write(root.join("app.rs"), &oversized).unwrap();
    fs::write(&input, "fn calc() {}").unwrap();
    let (handle, _) = selection(&root, "calc");
    assert!(!declaration(&root, &handle, &input, &["--write"]).0);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), oversized);
    fs::write(root.join("app.rs"), "fn calc() {}\n").unwrap();
    fs::write(&input, &oversized).unwrap();
    let (handle, _) = selection(&root, "calc");
    assert!(!declaration(&root, &handle, &input, &["--save-plan"]).0);
}

#[test]
fn declaration_syntax_acceptance_does_not_claim_callers_still_typecheck() {
    let source = "fn calc(n: i32) -> i32 { n + 1 }\nfn main() { let value: i32 = calc(3); println!(\"{}\", value); }\n";
    let (_temp, root, input) = fixture(source, b"fn calc(n: i32) -> bool { n > 0 }");
    assert_eq!(compiled_result(&root), b"4\n");
    let (handle, _) = selection(&root, "calc");
    let (success, report) = declaration(&root, &handle, &input, &["--write"]);
    assert!(success, "{report}");
    assert_eq!(report["validation"], "reparse-strict");
    assert_eq!(report["behavior_checked"], false);
    let checked = Command::new("rustc")
        .args(["--edition=2021", "--emit=metadata", "-o"])
        .arg(root.parent().unwrap().join("checked.rmeta"))
        .arg(root.join("app.rs"))
        .output()
        .unwrap();
    assert!(!checked.status.success());
    assert!(String::from_utf8_lossy(&checked.stderr).contains("E0308"));
    let id = report["transaction"].as_u64().unwrap().to_string();
    ok(&root, &["history", "undo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
}

fn insert_declaration(root: &Path, handle: &str, input: &Path, flags: &[&str]) -> (bool, Value) {
    let mut args = vec![
        "author",
        "insert-declaration",
        handle,
        "--from",
        input.to_str().unwrap(),
    ];
    args.extend(flags);
    run(root, &args)
}

#[test]
fn insertion_preserves_file_bytes_and_runs_new_function_through_saved_history() {
    let source = "// π\r\nfn seed(n: i32) -> i32 { n + 1 }\r\n// Keep this final comment.";
    let added = "fn main() { println!(\"{}\", seed(3)); }";
    let (_temp, root, input) = fixture(source, added.as_bytes());
    let (handle, _) = selection(&root, "app.rs");
    let (success, preview) = insert_declaration(&root, &handle, &input, &["--diff-bytes", "0"]);
    assert!(success, "{preview}");
    assert_eq!(preview["query"], "insert-declaration");
    assert_eq!(preview["schema"], "fr-author-1");
    assert_eq!(preview["insertion"]["before_span"]["start"], source.len());
    assert_eq!(preview["insertion"]["before_span"]["end"], source.len());
    assert_eq!(preview["insertion"]["leading_separator"], "\r\n");
    assert_eq!(preview["insertion"]["trailing_separator"], "\r\n");
    assert_eq!(preview["declaration"]["span"]["start"], source.len() + 2);
    assert_eq!(preview["name_resolution_checked"], false);
    assert_eq!(preview["diff"]["text"], "");
    assert!(!root.join(".fr-history").exists());
    let (success, saved) = insert_declaration(&root, &handle, &input, &["--save-plan"]);
    assert!(success, "{saved}");
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    let id = saved["transaction"].as_u64().unwrap().to_string();
    fs::write(&input, b"fn changed() {}").unwrap();
    ok(&root, &["history", "apply", &id, "--write"]);
    let expected = format!("{source}\r\n{added}\r\n");
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), expected);
    assert_eq!(compiled_result(&root), b"4\n");
    assert!(!insert_declaration(&root, &handle, &input, &["--write"]).0);
    fs::write(root.join("other.rs"), "fn other() {}\n").unwrap();
    ok(&root, &["history", "undo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    ok(&root, &["history", "patch", &id, "--check"]);
    assert!(ok(&root, &["history", "patch", &id])["patch"]
        .as_str()
        .unwrap()
        .contains(added));
    ok(&root, &["history", "redo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), expected);
    assert_eq!(
        fs::read_to_string(root.join("other.rs")).unwrap(),
        "fn other() {}\n"
    );
    fs::write(root.join("app.rs"), expected + "// Later change.\n").unwrap();
    assert!(!run(&root, &["history", "undo", &id, "--write"]).0);
}

#[test]
fn insertion_handles_empty_files_inner_attributes_comments_and_nested_names() {
    for source in [
        "",
        "\n",
        "// Final comment.",
        "//// Ordinary comment.",
        "/* Ordinary. */",
        "//! Crate documentation.\n#![allow(dead_code)]\n",
        "#!/usr/bin/env rust-script\n",
        "fn outer() { fn calc() {} }\n",
        "mod inner { pub fn calc() {} }\n",
        "#[inline]\nfn other() {}\n",
        "const TEXT: &str = \"/// A string.\";\n",
    ] {
        let (_temp, root, input) = fixture(source, b"\nfn calc() {}\n");
        let (handle, _) = selection(&root, "app.rs");
        let (success, report) = insert_declaration(&root, &handle, &input, &["--write"]);
        assert!(success, "{source:?}: {report}");
        let leading = if source.is_empty() || source.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        assert_eq!(
            fs::read_to_string(root.join("app.rs")).unwrap(),
            format!("{source}{leading}fn calc() {{}}\n")
        );
        let (fresh, _) = selection(&root, "app.rs");
        assert!(!insert_declaration(&root, &fresh, &input, &["--write"]).0);
    }
}

#[test]
fn insertion_refuses_duplicate_direct_names_and_unattached_outer_metadata() {
    for source in [
        "fn calc() {}\n",
        "fn r#calc() {}\n",
        "struct calc;\n",
        "type calc = i32;\n",
        "const calc: i32 = 1;\n",
        "mod calc {}\n",
        "trait calc {}\n",
        "#[cfg(any())]\nfn calc() {}\n",
        "macro_rules! calc { () => {} }\n",
        "fn other() {}\n/// Waiting documentation.\n",
        "fn other() {}\n/** Waiting documentation. */\n",
        "#[inline]\n",
        "fn other() {}\n#[inline]\n// Ordinary comment after attribute.\n",
        "/// Waiting documentation.\n// Ordinary comment.\n",
    ] {
        let (_temp, root, input) = fixture(source, b"fn calc() {}");
        let (handle, _) = selection(&root, "app.rs");
        let (success, report) = insert_declaration(&root, &handle, &input, &["--save-plan"]);
        assert!(!success, "{source}: {report}");
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
    let (_temp, root, input) = fixture("fn calc() {}\n", b"fn r#calc() {}");
    let (handle, _) = selection(&root, "app.rs");
    assert!(!insert_declaration(&root, &handle, &input, &["--write"]).0);
}

#[test]
fn insertion_refuses_bad_fragments_targets_revisions_and_conflicting_flags() {
    for text in [
        "",
        "fn calc();",
        "/// Documentation without a function.",
        "//! Inner documentation.\nfn calc() {}",
        "/*! Inner documentation. */ fn calc() {}",
        "// Ordinary comment.\nfn calc() {}",
        "/// Documentation.\n// Ordinary comment.\nfn calc() {}",
        "#[doc = \"Documentation.\"] fn calc() {}",
        "/// Documentation.\n#[inline] fn calc() {}",
        "fn calc() {} fn other() {}",
        "#[inline]\nfn calc() {}",
        "fn calc() {} // Trailing.",
        "struct calc;",
        "fn calc() { let x = ; }",
        "fn calc() {\0}",
    ] {
        let (_temp, root, input) = fixture("fn other() {}\n", text.as_bytes());
        let (handle, _) = selection(&root, "app.rs");
        assert!(!insert_declaration(&root, &handle, &input, &["--write"]).0);
        assert!(!root.join(".fr-history").exists());
    }
    for (file, source) in [
        ("app.ts", "function other() {}\n"),
        ("app.rs", "fn other() { let x = ; }\n"),
    ] {
        let (_temp, root, input) = fixture_file(file, source, b"fn calc() {}");
        let (handle, _) = selection(&root, file);
        assert!(!insert_declaration(&root, &handle, &input, &["--write"]).0);
    }
    let (_temp, root, input) = fixture("fn other() {}\n", b"fn calc() {}");
    let (function, _) = selection(&root, "other");
    assert!(!insert_declaration(&root, &function, &input, &["--write"]).0);
    let (handle, revision) = selection(&root, "app.rs");
    let short = handle.rsplit(':').next().unwrap();
    assert!(!insert_declaration(&root, short, &input, &["--write"]).0);
    assert!(insert_declaration(&root, short, &input, &["--revision", &revision]).0);
    assert!(!insert_declaration(&root, &handle, &input, &["--write", "--save-plan"]).0);
    assert!(!insert_declaration(&root, &handle, &input, &["--diff-bytes", "65537"]).0);
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"changed\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    assert!(!insert_declaration(&root, &handle, &input, &["--write"]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn insertion_bounds_fragment_bytes_and_reports_separators_separately() {
    let header = "fn calc() ";
    let text = format!("{header}{{{}}}", " ".repeat(65536 - header.len() - 2));
    let source = "// First.\r\n// Last.";
    let (_temp, root, input) = fixture(source, text.as_bytes());
    let (handle, _) = selection(&root, "app.rs");
    let (success, report) =
        insert_declaration(&root, &handle, &input, &["--diff-bytes", "0", "--write"]);
    assert!(success, "{report}");
    assert_eq!(report["declaration"]["bytes"], 65536);
    assert_eq!(report["insertion"]["added_bytes"], 65540);
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        format!("{source}\r\n{text}\r\n")
    );
    fs::write(root.join("app.rs"), source).unwrap();
    fs::write(&input, text + " ").unwrap();
    let (handle, _) = selection(&root, "app.rs");
    assert!(!insert_declaration(&root, &handle, &input, &["--save-plan"]).0);
}

#[test]
fn inserted_outer_documentation_compiles_under_missing_docs_and_travels_with_history() {
    for documentation in [
        "/// Returns π plus one.\r\n/// Keeps the argument.\r\n",
        "/** Returns the next value. */\n",
        "/// Returns the next value.\n/** Accepts any i32. */\n\n",
    ] {
        let source = "//! Fixture.\r\n#![deny(missing_docs)]\r\nfn main() { assert_eq!(calc(3), 4); }\r\n// Keep this final comment.";
        let function = "pub fn calc(n: i32) -> i32 { n + 1 }";
        let added = format!("{documentation}{function}");
        let (_temp, root, input) = fixture(source, added.as_bytes());
        let (handle, _) = selection(&root, "app.rs");
        let (success, report) = insert_declaration(&root, &handle, &input, &["--save-plan"]);
        assert!(success, "{report}");
        assert_eq!(report["signature"]["text"], "pub fn calc(n: i32) -> i32");
        assert_eq!(report["documentation"]["kind"], "outer-doc-comments");
        assert_eq!(report["documentation"]["bytes"], documentation.len());
        assert_eq!(report["documentation"]["span"]["start"], source.len() + 2);
        assert_eq!(
            report["documentation"]["span"]["end"],
            source.len() + 2 + documentation.len()
        );
        assert_eq!(report["declaration"]["bytes"], added.len());
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
        fs::write(&input, "fn different() {}\n").unwrap();
        ok(&root, &["history", "apply", "1", "--write", "--no-diff"]);
        let expected = format!("{source}\r\n{added}\r\n");
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), expected);
        assert!(compiled_result(&root).is_empty());
        let patch = ok(&root, &["history", "patch", "1"]);
        assert!(patch["patch"]
            .as_str()
            .unwrap()
            .contains(documentation.lines().next().unwrap()));
        fs::write(root.join("unrelated.rs"), "fn later() {}\n").unwrap();
        ok(&root, &["history", "undo", "1", "--write"]);
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
        ok(&root, &["history", "redo", "1", "--write"]);
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), expected);
        assert_eq!(
            fs::read_to_string(root.join("unrelated.rs")).unwrap(),
            "fn later() {}\n"
        );
    }
}

#[test]
fn insertion_counts_documentation_in_the_fragment_budget_without_hiding_the_signature() {
    let function = "pub fn calc() {}";
    let documentation = format!("/// {}\n", "x".repeat(65536 - function.len() - 5));
    let added = format!("{documentation}{function}");
    assert_eq!(added.len(), 65536);
    let (_temp, root, input) = fixture("", added.as_bytes());
    let (handle, _) = selection(&root, "app.rs");
    let (success, report) = insert_declaration(&root, &handle, &input, &["--diff-bytes", "0"]);
    assert!(success, "{report}");
    assert_eq!(report["signature"]["text"], "pub fn calc()");
    assert_eq!(report["documentation"]["bytes"], documentation.len());
    assert_eq!(report["declaration"]["bytes"], 65536);
    fs::write(&input, format!(" {added}")).unwrap();
    assert!(!insert_declaration(&root, &handle, &input, &["--save-plan"]).0);
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn wrapped_function_bindings_preserve_assertions_comments_and_neighboring_bodies() {
    for extension in ["ts", "tsx"] {
        for source in [
            "export const calc = (() => { return 1; });\n",
            "const calc = ((/* before */ (() => { return 1; }) /* after */));\n",
            "const calc = (() => { return 1; }) as () => number;\n",
            "const calc = (() => { return 1; }) satisfies () => number;\n",
            "const calc = (/* before */ function named() { return 1; } /* after */)!;\n",
            "const calc = (((() => { return 1; }) as /* type */ (() => number)) satisfies (() => number))!;\n",
            "const before = () => { return 0; }, calc = ((function () { return 1; }) as () => number), after = () => { return 3; };\n",
            "class C { readonly calc = ((() => { return 1; }) satisfies () => number); }\n",
            "class C { static calc = (function named() { return 1; })!; }\n",
            "function outer() { const calc = ((() => { return 1; }) as () => number); return calc(); }\n",
            "const calc = (async <T,>(n: T): Promise<number> => { return 1; }) satisfies <T>(n: T) => Promise<number>;\n",
        ] {
            let file = format!("app.{extension}");
            let old = "{ return 1; }";
            let new = "{ /* π */ return 2; }";
            let (_temp, root, input) = fixture_file(&file, source, new.as_bytes());
            let found = ok(&root, &["project", "find", "calc", "--in", &file, "--locals", "--source"]);
            assert_eq!(found["page"]["total"], 1);
            let columns = found["columns"].as_array().unwrap();
            let handle_column = columns.iter().position(|c| c == "handle").unwrap();
            let handle = found["rows"][0][handle_column].as_str().unwrap();
            let (success, report) = replace(&root, handle, &input, &["--write"]);
            assert!(success, "{file}: {source}: {report}");
            let start = report["body"]["before_span"]["start"].as_u64().unwrap() as usize;
            let end = report["body"]["before_span"]["end"].as_u64().unwrap() as usize;
            assert_eq!(&source[start..end], old);
            assert!(!report["signature"]["text"].as_str().unwrap().contains("return 1"));
            assert!(!report["signature"]["text"].as_str().unwrap().contains("return 0"));
            assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source.replace(old, new));
        }
    }
    for source in [
        "const calc = <() => number>(() => { return 1; });\n",
        "const calc = < /* type */ () => number > /* value */ (function named() { return 1; });\n",
        "const calc = ((<() => number>(() => { return 1; })) satisfies () => number)!;\n",
    ] {
        let (_temp, root, input) = fixture_file("app.ts", source, b"{ return 2; }");
        let (handle, _) = selection(&root, "calc");
        let (success, report) = replace(&root, &handle, &input, &["--write"]);
        assert!(success, "{source}: {report}");
        assert_eq!(
            fs::read_to_string(root.join("app.ts")).unwrap(),
            source.replace("{ return 1; }", "{ return 2; }")
        );
    }
}

#[test]
fn wrapped_bodies_compile_and_run_through_saved_history() {
    for (file, source, old, new) in [
        ("app.ts", "class C { base = 3; calc = ((((): number => { return this.base + 1; }) satisfies () => number) as () => number)!; }\nconst detached = new C().calc; console.log(detached());\n", "{ return this.base + 1; }", "{ return this.base * 2; }"),
        ("app.ts", "const calc = (<(n: number) => number>(function self(n: number): number { return n === 0 ? 1 : self(n - 1) + 1; }));\nconsole.log(calc(3));\n", "{ return n === 0 ? 1 : self(n - 1) + 1; }", "{ return n === 0 ? 0 : self(n - 1) + 2; }"),
        ("app.ts", "const calc = ((function* (n: number): Generator<number> { yield n + 1; }) satisfies (n: number) => Generator<number>);\nconsole.log(calc(3).next().value);\n", "{ yield n + 1; }", "{ yield n * 2; }"),
        ("app.tsx", concat!("declare namespace JSX { type Element = string; interface IntrinsicElements { span: {}; } }\n", "function h(tag: string, props: unknown, child: unknown): string { return String(child); }\n", "const calc = (((value: number): JSX.Element => { return <span>{value + 1}</span>; }) satisfies (value: number) => JSX.Element);\nconsole.log(calc(3));\n"), "{ return <span>{value + 1}</span>; }", "{ return <span>{value * 2}</span>; }"),
    ] {
        let (_temp, root, input) = fixture_file(file, source, new.as_bytes());
        assert_eq!(typescript_result(&root, file), b"4\n");
        let (handle, _) = selection(&root, "calc");
        let (success, saved) = replace(&root, &handle, &input, &["--save-plan"]);
        assert!(success, "{saved}");
        assert_eq!(saved["behavior_checked"], false);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
        let id = saved["transaction"].as_u64().unwrap().to_string();
        fs::write(&input, b"{}").unwrap();
        ok(&root, &["history", "apply", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source.replace(old, new));
        assert_eq!(typescript_result(&root, file), b"6\n");
        assert!(!replace(&root, &handle, &input, &["--save-plan"]).0);
        ok(&root, &["history", "undo", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
        assert_eq!(typescript_result(&root, file), b"4\n");
        ok(&root, &["history", "patch", &id, "--check"]);
        let patch = ok(&root, &["history", "patch", &id]);
        assert!(patch["patch"].as_str().unwrap().contains(new));
        ok(&root, &["history", "redo", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source.replace(old, new));
        assert_eq!(typescript_result(&root, file), b"6\n");
    }
}

#[test]
fn a_changed_initializer_wrapper_invalidates_handles_and_saved_plans() {
    let source = "const calc = (() => { return 1; }) as () => number;\n";
    let (_temp, root, input) = fixture_file("app.ts", source, b"{ return 2; }");
    let (handle, _) = selection(&root, "calc");
    let (success, saved) = replace(&root, &handle, &input, &["--save-plan"]);
    assert!(success, "{saved}");
    let changed = source.replace(" as ", " satisfies ");
    fs::write(root.join("app.ts"), &changed).unwrap();
    assert!(!replace(&root, &handle, &input, &["--write"]).0);
    let id = saved["transaction"].as_u64().unwrap().to_string();
    assert!(!run(&root, &["history", "apply", &id, "--write"]).0);
    assert_eq!(fs::read_to_string(root.join("app.ts")).unwrap(), changed);
}

#[test]
fn module_insertion_preserves_bytes_and_limits_name_checks_to_selected_scope() {
    for (source, prefix, suffix) in [
        ("mod target {}", "mod target {", "}"),
        ("mod target {\n    }\n", "mod target {\n", "    }\n"),
        (
            "mod outer {\r\n\tmod target {\r\n\t\t}\r\n}\r\n",
            "mod outer {\r\n\tmod target {\r\n",
            "\t\t}\r\n}\r\n",
        ),
        ("mod target { /* } */ }", "mod target { /* } */ ", "}"),
        (
            "mod target {\n// } final comment\n}\n",
            "mod target {\n// } final comment\n",
            "}\n",
        ),
        (
            "mod target {\n//! Inner docs.\n#![allow(dead_code)]\n}\n",
            "mod target {\n//! Inner docs.\n#![allow(dead_code)]\n",
            "}\n",
        ),
        (
            "fn calc() {} mod sibling { fn calc() {} } mod target { mod nested { fn calc() {} } }",
            "fn calc() {} mod sibling { fn calc() {} } mod target { mod nested { fn calc() {} } ",
            "}",
        ),
        (
            "mod r#target { const TEXT: &str = r#\"}\"#; }",
            "mod r#target { const TEXT: &str = r#\"}\"#; ",
            "}",
        ),
    ] {
        let added = "/// Keeps π and literal indentation.\npub fn calc() -> &'static str { r#\"first\n    second\"# }";
        let (_temp, root, input) = fixture(source, added.as_bytes());
        let name = if source.starts_with("mod r#") {
            "r#target"
        } else {
            "target"
        };
        let (handle, _) = selection(&root, name);
        let (success, report) = insert_declaration(&root, &handle, &input, &["--write"]);
        assert!(success, "{source:?}: {report}");
        let newline = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let leading = if prefix.ends_with('\n') { "" } else { newline };
        let expected = format!("{prefix}{leading}{added}{newline}{suffix}");
        let changed = fs::read_to_string(root.join("app.rs")).unwrap();
        assert_eq!(changed, expected);
        assert_eq!(report["container"]["kind"], "inline-module");
        let container = &report["container"]["before_span"];
        let body = &source[container["start"].as_u64().unwrap() as usize
            ..container["end"].as_u64().unwrap() as usize];
        assert!(body.starts_with('{') && body.ends_with('}'));
        assert_eq!(report["insertion"]["before_span"]["start"], prefix.len());
        assert_eq!(report["insertion"]["before_span"]["end"], prefix.len());
        let span = &report["declaration"]["span"];
        assert_eq!(
            &changed
                [span["start"].as_u64().unwrap() as usize..span["end"].as_u64().unwrap() as usize],
            added
        );
        let doc = &report["documentation"]["span"];
        assert_eq!(
            &changed
                [doc["start"].as_u64().unwrap() as usize..doc["end"].as_u64().unwrap() as usize],
            "/// Keeps π and literal indentation.\n"
        );
        let end = report["insertion"]["after_span"]["end"].as_u64().unwrap() as usize;
        assert_eq!(
            format!("{}{}", &changed[..prefix.len()], &changed[end..]),
            source
        );
        assert_eq!(report["name_resolution_checked"], false);
        assert!(report["name_check"]
            .as_str()
            .unwrap()
            .contains("selected module"));
    }
}

#[test]
fn module_insertion_compiles_private_scope_and_preserves_saved_history() {
    let source = "//! Fixture.\r\n#![deny(missing_docs)]\r\nmod api {\r\n    pub mod target {\r\n        fn seed(n: i32) -> i32 { n + 1 }\r\n    }\r\n}\r\nfn main() { println!(\"{}\", api::target::calc(3)); }\r\n";
    let added = "/// Returns the next seed.\npub fn calc(n: i32) -> i32 { seed(n) + 1 }";
    let (_temp, root, input) = fixture(source, added.as_bytes());
    let (handle, _) = selection(&root, "target");
    let (success, preview) = insert_declaration(&root, &handle, &input, &[]);
    assert!(success, "{preview}");
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    assert!(!root.join(".fr-history").exists());
    let (success, saved) = insert_declaration(&root, &handle, &input, &["--save-plan"]);
    assert!(success, "{saved}");
    let id = saved["transaction"].as_u64().unwrap().to_string();
    fs::write(&input, "fn changed() {}\n").unwrap();
    ok(&root, &["history", "apply", &id, "--write"]);
    let expected = source.replace("    }\r\n}", &format!("{added}\r\n    }}\r\n}}"));
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), expected);
    assert_eq!(compiled_result(&root), b"5\n");
    assert!(!insert_declaration(&root, &handle, &input, &["--write"]).0);
    fs::write(root.join("other.rs"), "fn other() {}\n").unwrap();
    ok(&root, &["history", "undo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
    ok(&root, &["history", "patch", &id, "--check"]);
    let patch = ok(&root, &["history", "patch", &id]);
    for line in added.lines() {
        assert!(patch["patch"]
            .as_str()
            .unwrap()
            .contains(&format!("+{line}")));
    }
    ok(&root, &["history", "redo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), expected);
    assert_eq!(compiled_result(&root), b"5\n");
    assert_eq!(
        fs::read_to_string(root.join("other.rs")).unwrap(),
        "fn other() {}\n"
    );
    fs::write(root.join("app.rs"), expected + "// Later change.\n").unwrap();
    assert!(!run(&root, &["history", "undo", &id, "--write"]).0);
}

#[test]
fn module_insertion_refuses_duplicates_dangling_metadata_and_unsupported_targets() {
    for content in [
        "fn calc() {}",
        "fn r#calc() {}",
        "struct calc;",
        "type calc = i32;",
        "const calc: i32 = 1;",
        "mod calc {}",
        "trait calc {}",
        "#[cfg(any())] fn calc() {}",
        "macro_rules! calc { () => {} }",
        "fn other() {}\n/// Waiting docs.\n",
        "fn other() {}\n/** Waiting docs. */",
        "#[inline]\n",
        "fn other() {}\n#[inline]\n// Ordinary comment.\n",
    ] {
        let source = format!("mod target {{ {content} }}\n");
        let (_temp, root, input) = fixture(&source, b"fn calc() {}");
        let (handle, _) = selection(&root, "target");
        let (success, report) = insert_declaration(&root, &handle, &input, &["--save-plan"]);
        assert!(!success, "{source}: {report}");
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
    for (source, selected) in [
        ("mod target;", "target"),
        ("mod outer { fn target() {} }", "target"),
        ("mod outer { trait target {} }", "target"),
        (
            "mod outer { struct Target; impl Target { fn target() {} } }",
            "target",
        ),
    ] {
        let (_temp, root, input) = fixture(source, b"fn calc() {}");
        let (handle, _) = selection(&root, selected);
        assert!(!insert_declaration(&root, &handle, &input, &["--write"]).0);
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
}

#[test]
fn module_insertion_distinguishes_same_named_modules_and_requires_current_revision() {
    let source = concat!(
        "mod left { mod target {} }\n",
        "mod right { mod target {} }\n"
    );
    for selected in 0..2 {
        let (_temp, root, input) = fixture(source, b"fn calc() {}");
        let map = ok(
            &root,
            &["project", "map", "--depth", "64", "--fields", "handle,name"],
        );
        let rows: Vec<_> = map["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row[1] == "target")
            .collect();
        assert_eq!(rows.len(), 2);
        let handle = rows[selected][0].as_str().unwrap();
        let short = handle.rsplit(':').next().unwrap();
        assert!(!insert_declaration(&root, short, &input, &["--write"]).0);
        let (success, report) = insert_declaration(
            &root,
            short,
            &input,
            &[
                "--revision",
                map["revision"].as_str().unwrap(),
                "--save-plan",
            ],
        );
        assert!(success, "{report}");
        let offset = source.match_indices("{}").nth(selected).unwrap().0 + 1;
        assert_eq!(report["insertion"]["before_span"]["start"], offset);
        ok(&root, &["history", "apply", "1", "--write"]);
        assert_eq!(
            fs::read_to_string(root.join("app.rs")).unwrap(),
            format!(
                "{}\nfn calc() {{}}\n{}",
                &source[..offset],
                &source[offset..]
            )
        );
    }
    for fault in ["source", "manifest"] {
        let (_temp, root, input) = fixture(source, b"fn calc() {}");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"fixture\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        let (handle, _) = selection(&root, "target");
        assert!(insert_declaration(&root, &handle, &input, &["--save-plan"]).0);
        if fault == "source" {
            fs::write(root.join("app.rs"), source.replace("left", "renamed")).unwrap();
        } else {
            fs::write(
                root.join("Cargo.toml"),
                "[package]\nname=\"changed\"\nversion=\"0.1.0\"\n",
            )
            .unwrap();
        }
        assert!(!insert_declaration(&root, &handle, &input, &["--write"]).0);
        let (applied, report) = run(&root, &["history", "apply", "1", "--write"]);
        assert_eq!(applied, fault == "manifest", "{fault}: {report}");
        if fault == "manifest" {
            assert!(fs::read_to_string(root.join("Cargo.toml"))
                .unwrap()
                .contains("changed"));
        }
    }
}

#[test]
fn module_insertion_bounds_fragment_bytes_and_hashes_the_actual_splice() {
    use sha2::{Digest, Sha256};
    let text = format!("fn calc() {{{}}}", " ".repeat(65536 - 12));
    assert_eq!(text.len(), 65536);
    let source = "// π\r\nmod target {}\r\n";
    let (_temp, root, input) = fixture(source, text.as_bytes());
    let (handle, _) = selection(&root, "target");
    let (success, report) =
        insert_declaration(&root, &handle, &input, &["--write", "--diff-bytes", "0"]);
    assert!(success, "{report}");
    assert_eq!(report["insertion"]["added_bytes"], 65540);
    let changed = fs::read_to_string(root.join("app.rs")).unwrap();
    let span = &report["insertion"]["after_span"];
    let splice =
        &changed[span["start"].as_u64().unwrap() as usize..span["end"].as_u64().unwrap() as usize];
    assert_eq!(splice, format!("\r\n{text}\r\n"));
    assert_eq!(
        report["insertion"]["sha256"],
        format!("{:x}", Sha256::digest(splice.as_bytes()))
    );
    ok(&root, &["history", "undo", "1", "--write"]);
    fs::write(&input, text + " ").unwrap();
    assert!(!insert_declaration(&root, &handle, &input, &["--save-plan"]).0);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
}

fn compiled_go_result(root: &Path) -> Vec<u8> {
    let binary = root.parent().unwrap().join("compiled-go");
    let output = Command::new("go")
        .args(["build", "-o"])
        .arg(&binary)
        .arg("app.go")
        .current_dir(root)
        .env(
            "GOCACHE",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/go-cache"),
        )
        .env("GOTOOLCHAIN", "local")
        .env("GOWORK", "off")
        .env("GO111MODULE", "off")
        .env("GOPROXY", "off")
        .env("CGO_ENABLED", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(binary).output().unwrap();
    assert!(output.status.success());
    output.stdout
}

#[test]
fn go_body_replacement_preserves_function_and_receiver_headers() {
    for (name, source) in [
        ("calc", concat!("// π\r\npackage main\r\n", "//go:noinline\r\nfunc calc() int { return 1 }\r\n")),
        ("calc", "package main\nfunc calc[T ~int](n T) int { return 1 }\n"),
        ("calc", "package main\ntype Counter int\nfunc (c Counter) calc() int { return 1 }\n"),
        ("calc", "package main\ntype Counter int\nfunc (c *Counter) calc() int { return 1 }\n"),
        ("calc", "package main\ntype Box[T any] struct { value T }\nfunc (b *Box[T]) calc() (result int) { return 1 }\n"),
        ("calc", "package main\nfunc calc() int /* before */ { return 1 }; /* after */\n"),
        ("計算", "package main\nfunc 計算() int { return 1 }\n"),
    ] {
        let body = "{ return 2 }";
        let (_temp, root, input) = fixture_file("app.go", source, body.as_bytes());
        let (handle, _) = selection(&root, name);
        let (success, report) = replace(&root, &handle, &input, &["--write"]);
        assert!(success, "{source}: {report}");
        let changed = fs::read_to_string(root.join("app.go")).unwrap();
        assert_eq!(changed, source.replace("{ return 1 }", body));
        let span = &report["body"]["after_span"];
        assert_eq!(&changed[span["start"].as_u64().unwrap() as usize..span["end"].as_u64().unwrap() as usize], body);
        assert_eq!(report["validation"], "reparse-strict");
        assert_eq!(report["behavior_checked"], false);
    }
}

#[test]
fn go_saved_bodies_compile_through_apply_undo_patch_and_redo() {
    for (declarations, name, call, old, new, before, after) in [
        (
            "func calc[T ~int](n T) T OLD",
            "calc",
            "calc(3)",
            "{ return n + 1 }",
            "{ return n * 2 }",
            "4\n",
            "6\n",
        ),
        (
            concat!(
                "type Counter struct { base int }; ",
                "func (c *Counter) calc(n int) int OLD"
            ),
            "calc",
            "(&Counter{base: 4}).calc(3)",
            "{ return c.base + n + 1 }",
            "{ return c.base + n * 2 }",
            "8\n",
            "10\n",
        ),
        (
            "func calc(n int) (result int) OLD",
            "calc",
            "calc(3)",
            "{ result = n + 1; return }",
            "{ defer func() { result++ }(); result = n * 2; return }",
            "4\n",
            "7\n",
        ),
        (
            "var result int; func init() OLD",
            "init",
            "result",
            "{ result = 4 }",
            "{ result = 6 }",
            "4\n",
            "6\n",
        ),
        (
            "func calc() string OLD",
            "calc",
            "calc()",
            "{ return \"old\" }",
            "{ return `first\n  π` }",
            "old\n",
            "first\n  π\n",
        ),
    ] {
        let source = format!("// π\r\npackage main\r\nimport \"fmt\"\r\n{}\r\nfunc main() {{ fmt.Println({call}) }}\r\n", declarations.replace("OLD", old));
        let (_temp, root, input) = fixture_file("app.go", &source, new.as_bytes());
        assert_eq!(compiled_go_result(&root), before.as_bytes());
        let (handle, _) = selection(&root, name);
        let (success, preview) = replace(&root, &handle, &input, &["--diff-bytes", "0"]);
        assert!(success, "{preview}");
        assert_eq!(preview["diff"]["text"], "");
        assert!(!root.join(".fr-history").exists());
        let (success, saved) = replace(&root, &handle, &input, &["--save-plan"]);
        assert!(success, "{saved}");
        let id = saved["transaction"].as_u64().unwrap().to_string();
        fs::write(&input, "{}").unwrap();
        ok(&root, &["history", "apply", &id, "--write"]);
        let expected = source.replace(old, new);
        assert_eq!(fs::read_to_string(root.join("app.go")).unwrap(), expected);
        assert_eq!(compiled_go_result(&root), after.as_bytes());
        assert!(!replace(&root, &handle, &input, &["--write"]).0);
        fs::write(root.join("unrelated.txt"), "retained\n").unwrap();
        ok(&root, &["history", "undo", &id, "--write"]);
        assert_eq!(fs::read_to_string(root.join("app.go")).unwrap(), source);
        assert_eq!(compiled_go_result(&root), before.as_bytes());
        ok(&root, &["history", "patch", &id, "--check"]);
        let patch = ok(&root, &["history", "patch", &id]);
        for line in new.lines() {
            assert!(patch["patch"].as_str().unwrap().contains(line));
        }
        ok(&root, &["history", "redo", &id, "--write"]);
        assert_eq!(compiled_go_result(&root), after.as_bytes());
        assert_eq!(
            fs::read_to_string(root.join("unrelated.txt")).unwrap(),
            "retained\n"
        );
        fs::write(root.join("app.go"), expected + "// Later edit.\n").unwrap();
        assert!(!run(&root, &["history", "undo", &id, "--write"]).0);
    }
}

#[test]
fn go_body_replacement_distinguishes_same_named_receivers() {
    let source = "package main\ntype Left int\ntype Right int\nfunc (Left) calc() int { return 1 }\nfunc (Right) calc() int { return 1 }\n";
    for selected in 0..2 {
        let (_temp, root, input) = fixture_file("app.go", source, b"{ return 2 }");
        let map = ok(
            &root,
            &["project", "map", "--depth", "64", "--fields", "handle,name"],
        );
        let rows: Vec<_> = map["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row[1] == "calc")
            .collect();
        assert_eq!(rows.len(), 2);
        let handle = rows[selected][0].as_str().unwrap();
        let (success, report) = replace(&root, handle, &input, &["--write"]);
        assert!(success, "{report}");
        let offset = source
            .match_indices("{ return 1 }")
            .nth(selected)
            .unwrap()
            .0;
        assert_eq!(report["body"]["before_span"]["start"], offset);
        let mut expected = source.to_owned();
        expected.replace_range(offset..offset + 12, "{ return 2 }");
        assert_eq!(fs::read_to_string(root.join("app.go")).unwrap(), expected);
    }
}

#[test]
fn go_body_replacement_refuses_nonfunctions_and_escaped_fragments() {
    for (source, name) in [
        ("package main\nvar calc = func() int { return 1 }\n", "calc"),
        (
            "package main\ntype Reader interface { calc() int }\n",
            "calc",
        ),
        ("package main\nfunc calc()\n", "calc"),
        ("package main\ntype calc struct {}\n", "calc"),
        (
            "package main\nfunc outer() { calc := func() {}; calc() }\n",
            "calc",
        ),
        (
            "package main\nfunc outer() { var calc int; _ = calc }\n",
            "calc",
        ),
        ("package main\nfunc calc() { x := }\n", "calc"),
        ("package main\nfunc calc() {}\n", "app.go"),
    ] {
        let (_temp, root, input) = fixture_file("app.go", source, b"{}");
        let (handle, _) = selection(&root, name);
        let (success, report) = replace(&root, &handle, &input, &["--save-plan"]);
        assert!(!success, "{source}: {report}");
        assert_eq!(fs::read_to_string(root.join("app.go")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
    for body in [
        "",
        "{",
        "{ x := }",
        "{}\nfunc escape() {}",
        "{} // trailing",
        "{};",
        "func() {}",
        "{\0}",
    ] {
        let source = "package main\nfunc calc() {}\n";
        let (_temp, root, input) = fixture_file("app.go", source, body.as_bytes());
        let (handle, _) = selection(&root, "calc");
        assert!(!replace(&root, &handle, &input, &["--write"]).0, "{body:?}");
        assert_eq!(fs::read_to_string(root.join("app.go")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
}

#[test]
fn go_body_replacement_keeps_revision_noop_and_size_guards() {
    let source = "package main\nfunc calc() {}\n";
    let (_temp, root, input) = fixture_file("app.go", source, b" \n{}\n");
    let (handle, revision) = selection(&root, "calc");
    let short = handle.rsplit(':').next().unwrap();
    assert!(!replace(&root, short, &input, &["--write"]).0);
    let (success, report) = replace(&root, short, &input, &["--revision", &revision, "--write"]);
    assert!(success, "{report}");
    assert_eq!(report["changed"], false);
    assert!(report["transaction"].is_null());
    assert!(!root.join(".fr-history").exists());
    let body = format!("{{{}}}", " ".repeat(65534));
    fs::write(&input, &body).unwrap();
    assert!(replace(&root, &handle, &input, &["--save-plan"]).0);
    fs::write(root.join("app.go"), source.replace("calc", "renamed")).unwrap();
    assert!(!run(&root, &["history", "apply", "1", "--write"]).0);
    assert!(!replace(&root, &handle, &input, &["--write"]).0);
    fs::write(root.join("app.go"), source).unwrap();
    ok(&root, &["history", "apply", "1", "--write"]);
    assert_eq!(
        fs::read_to_string(root.join("app.go")).unwrap(),
        source.replace("{}", &body)
    );
    let (fresh, _) = selection(&root, "calc");
    fs::write(&input, &body[1..]).unwrap();
    assert!(!replace(&root, &fresh, &input, &["--write"]).0);
    fs::write(&input, format!(" {body}")).unwrap();
    assert!(!replace(&root, &fresh, &input, &["--write"]).0);
    fs::write(
        root.join("app.go"),
        source.replace("{}", &format!("{{{}}}", " ".repeat(65535))),
    )
    .unwrap();
    fs::write(&input, "{}").unwrap();
    let (fresh, _) = selection(&root, "calc");
    assert!(!replace(&root, &fresh, &input, &["--write"]).0);
}

fn batch_step(root: &Path, op: &str, handle: &str, label: &str, text: &str) -> Value {
    let input = root.parent().unwrap().join(label);
    fs::write(&input, text).unwrap();
    serde_json::json!({"op":op,"handle":handle,"from":input})
}

fn batch_manifest(root: &Path, operations: Vec<Value>, revision: Option<&str>) -> PathBuf {
    let input = root.parent().unwrap().join("batch.json");
    let mut manifest = serde_json::json!({"operations":operations});
    if let Some(revision) = revision {
        manifest["revision"] = revision.into();
    }
    fs::write(&input, serde_json::to_vec(&manifest).unwrap()).unwrap();
    input
}

fn batch(root: &Path, input: &Path, flags: &[&str]) -> (bool, Value) {
    let mut args = vec!["author", "batch", "--from", input.to_str().unwrap()];
    args.extend(flags);
    run(root, &args)
}

#[test]
fn batch_coordinates_caller_signature_and_helper_through_one_saved_transaction() {
    let app = "mod calc;\nfn main() { println!(\"{}\", calc::evaluate(3)); }\n";
    let calc = "// π\r\npub fn evaluate(n: i32) -> i32 { n + 1 }\r\n";
    let (_temp, root, _) = fixture(app, b"{}");
    fs::write(root.join("calc.rs"), calc).unwrap();
    assert_eq!(compiled_result(&root), b"4\n");
    let (main, _) = selection(&root, "main");
    let (evaluate, _) = selection(&root, "evaluate");
    let (file, _) = selection(&root, "calc.rs");
    let declaration = "pub fn evaluate(n: i64, extra: i64) -> i64 { twice(n) + extra }";
    let body = "{ println!(\"{}\", calc::evaluate(3, 1)); }";
    let helper = "fn twice(n: i64) -> i64 { n * 2 }";
    let input = batch_manifest(
        &root,
        vec![
            batch_step(
                &root,
                "replace-declaration",
                &evaluate,
                "declaration.txt",
                declaration,
            ),
            batch_step(&root, "replace-body", &main, "caller.txt", body),
            batch_step(&root, "insert-declaration", &file, "helper.txt", helper),
        ],
        None,
    );
    let (success, preview) = batch(&root, &input, &["--diff-bytes", "0"]);
    assert!(success, "{preview}");
    assert_eq!(preview["schema"], "fr-author-batch-1");
    assert_eq!(preview["span_basis"], "original-source");
    assert_eq!(preview["files_changed"], 2);
    assert_eq!(preview["diff"]["text"], "");
    assert!(preview.get("coverage").is_some());
    assert_eq!(preview["steps"].as_array().unwrap().len(), 3);
    assert!(preview["steps"]
        .as_array()
        .unwrap()
        .iter()
        .all(|step| step.get("coverage").is_none() && step.get("after_span").is_none()));
    assert!(!root.join(".fr-history").exists());
    let (success, saved) = batch(&root, &input, &["--save-plan"]);
    assert!(success, "{saved}");
    let id = saved["transaction"].as_u64().unwrap().to_string();
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), app);
    assert_eq!(fs::read_to_string(root.join("calc.rs")).unwrap(), calc);
    fs::write(
        root.parent().unwrap().join("declaration.txt"),
        "fn changed() {}",
    )
    .unwrap();
    fs::write(&input, "{}").unwrap();
    ok(&root, &["history", "apply", &id, "--write"]);
    let changed_app = app.replace("{ println!(\"{}\", calc::evaluate(3)); }", body);
    let changed_calc = format!("// π\r\n{declaration}\r\n{helper}\r\n");
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        changed_app
    );
    assert_eq!(
        fs::read_to_string(root.join("calc.rs")).unwrap(),
        changed_calc
    );
    assert_eq!(compiled_result(&root), b"7\n");
    fs::write(root.join("unrelated.txt"), "retained\n").unwrap();
    ok(&root, &["history", "undo", &id, "--write"]);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), app);
    assert_eq!(fs::read_to_string(root.join("calc.rs")).unwrap(), calc);
    assert_eq!(compiled_result(&root), b"4\n");
    ok(&root, &["history", "patch", &id, "--check"]);
    let patch = ok(&root, &["history", "patch", &id]);
    assert!(patch["patch"].as_str().unwrap().contains("a/app.rs"));
    assert!(patch["patch"].as_str().unwrap().contains("a/calc.rs"));
    ok(&root, &["history", "redo", &id, "--write"]);
    assert_eq!(compiled_result(&root), b"7\n");
    assert_eq!(
        fs::read_to_string(root.join("unrelated.txt")).unwrap(),
        "retained\n"
    );
    fs::write(root.join("calc.rs"), changed_calc + "// Later edit.\n").unwrap();
    assert!(!run(&root, &["history", "undo", &id, "--write"]).0);
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        changed_app
    );
}

#[test]
fn batch_keeps_original_spans_when_same_file_edits_change_lengths() {
    use sha2::{Digest, Sha256};
    let source = "fn first() -> i32 { 1 }\nfn second() -> i32 { 2 }\n";
    let (_temp, root, _) = fixture(source, b"{}");
    let (first, _) = selection(&root, "first");
    let (second, _) = selection(&root, "second");
    let before = "{ 2 }";
    let after = "{ 200 }";
    let input = batch_manifest(
        &root,
        vec![
            batch_step(&root, "replace-body", &second, "second.txt", after),
            batch_step(
                &root,
                "replace-body",
                &first,
                "first.txt",
                "{ let value = 100; value }",
            ),
        ],
        None,
    );
    let (success, report) = batch(&root, &input, &["--write"]);
    assert!(success, "{report}");
    assert_eq!(report["files_changed"], 1);
    assert_eq!(
        report["steps"][0]["before_span"]["start"],
        source.find(before).unwrap()
    );
    assert_eq!(
        report["steps"][0]["after_sha256"],
        format!("{:x}", Sha256::digest(after.as_bytes()))
    );
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        source
            .replace("{ 1 }", "{ let value = 100; value }")
            .replace(before, after)
    );
}

#[test]
fn batch_refuses_overlaps_duplicate_selections_and_insertion_boundaries() {
    for case in [
        "same-body",
        "same-noop",
        "body-declaration",
        "nested",
        "same-insertion",
        "touching-insertion",
    ] {
        let source = if case == "touching-insertion" {
            "fn outer() {}"
        } else {
            "fn outer() { fn inner() {} }\n"
        };
        let (_temp, root, _) = fixture(source, b"{}");
        let (outer, _) = selection(&root, "outer");
        let (file, _) = selection(&root, "app.rs");
        let original_body = if case == "touching-insertion" {
            "{}"
        } else {
            "{ fn inner() {} }"
        };
        let first = batch_step(&root, "replace-body", &outer, "first.txt", original_body);
        let operations = match case {
            "same-body" | "same-noop" => vec![
                first,
                batch_step(
                    &root,
                    "replace-body",
                    &outer,
                    "second.txt",
                    if case == "same-noop" {
                        original_body
                    } else {
                        "{}"
                    },
                ),
            ],
            "body-declaration" => vec![
                first,
                batch_step(
                    &root,
                    "replace-declaration",
                    &outer,
                    "second.txt",
                    "fn outer() {}",
                ),
            ],
            "nested" => {
                let (inner, _) = selection(&root, "inner");
                vec![
                    first,
                    batch_step(
                        &root,
                        "replace-body",
                        &inner,
                        "second.txt",
                        "{ let x = 1; }",
                    ),
                ]
            }
            "same-insertion" => vec![
                batch_step(
                    &root,
                    "insert-declaration",
                    &file,
                    "first.txt",
                    "fn added() {}",
                ),
                batch_step(
                    &root,
                    "insert-declaration",
                    &file,
                    "second.txt",
                    "fn extra() {}",
                ),
            ],
            _ => vec![
                first,
                batch_step(
                    &root,
                    "insert-declaration",
                    &file,
                    "second.txt",
                    "fn added() {}",
                ),
            ],
        };
        let input = batch_manifest(&root, operations, None);
        let (success, report) = batch(&root, &input, &["--save-plan"]);
        assert!(!success, "{case}: {report}");
        assert!(
            report["error"]["message"]
                .as_str()
                .unwrap()
                .contains("overlap"),
            "{report}"
        );
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
}

#[test]
fn batch_refuses_a_late_bad_step_without_recording_earlier_changes() {
    for fault in [
        "syntax",
        "stale",
        "unsupported",
        "missing",
        "manifest-revision",
    ] {
        let source = "fn first() {}\nfn second() {}\n";
        let (_temp, root, _) = fixture(source, b"{}");
        let (first, revision) = selection(&root, "first");
        let (second, _) = selection(&root, "second");
        let mut bad = batch_step(&root, "replace-body", &second, "bad.txt", "{}");
        match fault {
            "syntax" => fs::write(root.parent().unwrap().join("bad.txt"), "{ let x = ; }").unwrap(),
            "stale" => bad["handle"] = second.replacen(&revision[..32], &"0".repeat(32), 1).into(),
            "unsupported" => bad["handle"] = selection(&root, "app.rs").0.into(),
            "missing" => fs::remove_file(root.parent().unwrap().join("bad.txt")).unwrap(),
            _ => {}
        }
        let input = batch_manifest(
            &root,
            vec![
                batch_step(&root, "replace-body", &first, "first.txt", "{ let x = 1; }"),
                bad,
            ],
            if fault == "manifest-revision" {
                Some("stale")
            } else {
                None
            },
        );
        let (success, report) = batch(&root, &input, &["--write"]);
        assert!(!success, "{fault}: {report}");
        assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), source);
        assert!(!root.join(".fr-history").exists());
    }
}

#[test]
fn batch_bounds_manifest_steps_and_reports_noops_without_history() {
    let source: String = (0..32).map(|n| format!("fn item{n}() {{}}\n")).collect();
    let (_temp, root, _) = fixture(&source, b"{}");
    let mut steps = Vec::new();
    let (_, revision) = selection(&root, "item0");
    for n in 0..32 {
        let (handle, _) = selection(&root, &format!("item{n}"));
        steps.push(batch_step(
            &root,
            "replace-body",
            if n % 2 == 0 {
                &handle
            } else {
                handle.rsplit(':').next().unwrap()
            },
            &format!("step{n}.txt"),
            "{}",
        ));
    }
    let input = batch_manifest(&root, steps.clone(), Some(&revision));
    let (success, report) = batch(&root, &input, &["--write"]);
    assert!(success, "{report}");
    assert_eq!(report["changed"], false);
    assert_eq!(report["files_changed"], 0);
    assert!(report["transaction"].is_null());
    assert_eq!(report["steps"].as_array().unwrap().len(), 32);
    assert!(!root.join(".fr-history").exists());
    let input = batch_manifest(&root, steps.clone(), None);
    assert!(!batch(&root, &input, &["--write"]).0);
    steps.push(steps[0].clone());
    let input = batch_manifest(&root, steps, Some(&revision));
    assert!(!batch(&root, &input, &["--write"]).0);
    for content in [
        "{}".to_owned(),
        "{\"operations\":[]}".to_owned(),
        " ".repeat(65537),
        "{\"operations\":[],\"unknown\":true}".to_owned(),
    ] {
        fs::write(&input, content).unwrap();
        assert!(!batch(&root, &input, &["--save-plan"]).0);
    }
    assert!(!root.join(".fr-history").exists());
}

#[test]
fn batch_combines_languages_and_resolves_fragments_from_workspace_root() {
    let rust = "mod target {\n}\n";
    let go = "package main\nfunc GoCalc() int { return 1 }\n";
    let tsx = "export const View = () => { return <span>old</span>; };\n";
    let (_temp, root, _) = fixture(rust, b"{}");
    fs::write(root.join("app.go"), go).unwrap();
    fs::write(root.join("app.tsx"), tsx).unwrap();
    let (target, _) = selection(&root, "target");
    let (go_handle, _) = selection(&root, "GoCalc");
    let (view, _) = selection(&root, "View");
    let mut go_step = batch_step(
        &root,
        "replace-body",
        &go_handle,
        "go-body.txt",
        "{ return 2 }",
    );
    go_step["from"] = "../go-body.txt".into();
    let input = batch_manifest(
        &root,
        vec![
            batch_step(
                &root,
                "insert-declaration",
                &target,
                "rust-function.txt",
                "fn added() {}",
            ),
            go_step,
            batch_step(
                &root,
                "replace-body",
                &view,
                "tsx-body.txt",
                "{ return <span>new</span>; }",
            ),
        ],
        None,
    );
    let nested = root.parent().unwrap().join("inputs");
    fs::create_dir(&nested).unwrap();
    let manifest = nested.join("batch.json");
    fs::rename(&input, &manifest).unwrap();
    let (success, report) = batch(&root, &manifest, &["--write"]);
    assert!(success, "{report}");
    assert_eq!(report["files_changed"], 3);
    assert_eq!(
        fs::read_to_string(root.join("app.rs")).unwrap(),
        "mod target {\nfn added() {}\n}\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("app.go")).unwrap(),
        go.replace("return 1", "return 2")
    );
    assert_eq!(
        fs::read_to_string(root.join("app.tsx")).unwrap(),
        tsx.replace("old", "new")
    );
    ok(&root, &["history", "undo", "1", "--write"]);
    for (file, source) in [("app.rs", rust), ("app.go", go), ("app.tsx", tsx)] {
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
    }
}

#[test]
fn batch_rejects_unknown_fields_inputs_flags_and_stale_saved_files() {
    let source = "fn first() {}\nfn second() {}\n";
    let (_temp, root, _) = fixture(source, b"{}");
    let (first, _) = selection(&root, "first");
    let (second, _) = selection(&root, "second");
    let step = batch_step(&root, "replace-body", &first, "first.txt", "{ let x = 1; }");
    for fault in [
        "unknown-field",
        "unknown-op",
        "top-field",
        "duplicate-field",
    ] {
        let mut bad = step.clone();
        match fault {
            "unknown-field" => bad["write"] = true.into(),
            "unknown-op" => bad["op"] = "delete".into(),
            _ => {}
        }
        let input = batch_manifest(&root, vec![bad], None);
        if fault == "top-field" {
            let mut value: Value = serde_json::from_slice(&fs::read(&input).unwrap()).unwrap();
            value["write"] = true.into();
            fs::write(&input, value.to_string()).unwrap();
        } else if fault == "duplicate-field" {
            let value = fs::read_to_string(&input)
                .unwrap()
                .replacen("{", "{\"operations\":[],", 1);
            fs::write(&input, value).unwrap();
        }
        assert!(!batch(&root, &input, &["--write"]).0, "{fault}");
    }
    let input = batch_manifest(
        &root,
        vec![
            step,
            batch_step(
                &root,
                "replace-body",
                &second,
                "second.txt",
                "{ let x = 2; }",
            ),
        ],
        None,
    );
    assert!(!batch(&root, &input, &["--save-plan", "--write"]).0);
    assert!(!batch(&root, &input, &["--diff-bytes", "65537"]).0);
    #[cfg(unix)]
    {
        let linked = root.parent().unwrap().join("linked-batch.json");
        std::os::unix::fs::symlink(&input, &linked).unwrap();
        assert!(!batch(&root, &linked, &["--write"]).0);
    }
    assert!(!root.join(".fr-history").exists());
    let (success, saved) = batch(&root, &input, &["--save-plan"]);
    assert!(success, "{saved}");
    let changed = source.replace("second", "renamed");
    fs::write(root.join("app.rs"), &changed).unwrap();
    assert!(!run(&root, &["history", "apply", "1", "--write"]).0);
    assert!(!batch(&root, &input, &["--write"]).0);
    assert_eq!(fs::read_to_string(root.join("app.rs")).unwrap(), changed);
}
