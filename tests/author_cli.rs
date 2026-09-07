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
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("app.rs"), source).unwrap();
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
