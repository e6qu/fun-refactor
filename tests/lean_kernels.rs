use fun_refactor::edit::{apply_to_string, Edit, EditSet};
use fun_refactor::index::Index;
use fun_refactor::refactor::{extract, imports, inline, move_symbol, rename, signature};
use fun_refactor::scan::{scan, ScanOptions};
use fun_refactor::span::{LineCol, LineIndex, Span};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Once;

static KERNEL_IS_BUILT: Once = Once::new();

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn build_kernel() {
    KERNEL_IS_BUILT.call_once(|| {
        let output = Command::new("lake")
            .args(["build", "--wfail"])
            .current_dir(root().join("kernels"))
            .output()
            .expect("Lean is installed for the kernel gate");
        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    });
}

fn edits_for(source: &str) -> Vec<(usize, usize, &'static str)> {
    let mut edits = Vec::new();
    for start in 0..=source.len() {
        for width in 0..=source.len() - start {
            for replacement in ["", "X", "YZ", "λ"] {
                edits.push((start, start + width, replacement));
            }
        }
    }
    edits
}

fn plans_for(source: &str) -> Vec<Vec<(usize, usize, &'static str)>> {
    let edits = edits_for(source);
    let mut plans = vec![Vec::new()];
    plans.extend(edits.iter().copied().map(|edit| vec![edit]));
    for first in &edits {
        for second in &edits {
            plans.push(vec![*first, *second]);
        }
    }
    plans.push(vec![(source.len(), source.len() + 1, "X")]);
    plans
}

fn position_sources() -> Vec<String> {
    let alphabet = ["a", "é", "\n", "名"];
    let mut sources = vec![String::new()];
    let mut words = sources.clone();
    for _ in 0..4 {
        words = words
            .iter()
            .flat_map(|prefix| {
                alphabet
                    .iter()
                    .map(move |character| format!("{prefix}{character}"))
            })
            .collect();
        sources.extend(words.clone());
    }
    sources
}

fn lean_string(value: &str) -> String {
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => write!(out, "\\u{{{:x}}}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('\"');
    out
}

fn kernel_accepts_all(plans: &[(&str, &[Edit], &str)]) {
    build_kernel();
    let mut program = String::from(
        "-- fn kernel program\nimport FrKernels.Edit\n\nset_option maxRecDepth 5000\nset_option maxHeartbeats 1000000\n\nopen FrKernels\n",
    );
    for (number, (source, edits, expected)) in plans.iter().enumerate() {
        let edits = edits
            .iter()
            .map(|edit| {
                format!(
                    "-- fn generated edit\n{{ start := {}, stop := {}, replacement := {} }}",
                    edit.span.start,
                    edit.span.end,
                    lean_string(&edit.replacement),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            program,
            "\n-- fn generated plan\ndef source_{number} : String := {}\ndef edits_{number} : List Edit := [{edits}]\n\nexample : applyChecked source_{number} edits_{number} = some {} := by decide\n",
            lean_string(source),
            lean_string(expected),
        )
        .unwrap();
    }
    let dir = tempfile::Builder::new()
        .prefix(".fr-kernel-")
        .tempdir_in(root().join("kernels"))
        .expect("temporary Lean plan in the kernel package");
    let file = dir.path().join("plan.lean");
    std::fs::write(&file, program).expect("write Lean plan");
    let output = Command::new("lake")
        .args(["env", "lean", file.to_str().expect("UTF-8 temporary path")])
        .current_dir(root().join("kernels"))
        .output()
        .expect("Lean is installed for the kernel gate");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn kernel_accepts(source: &str, edits: &[Edit], expected: &str) {
    kernel_accepts_all(&[(source, edits, expected)]);
}

#[test]
fn the_edit_kernel_accepts_a_reported_declaration_replacement() {
    reported_declaration_plan("replace-declaration", "calc", false);
}

#[test]
fn the_edit_kernel_accepts_a_reported_declaration_insertion() {
    reported_declaration_plan("insert-declaration", "app.rs", false);
}

#[test]
fn the_edit_kernel_accepts_a_reported_module_insertion() {
    reported_declaration_plan("insert-declaration", "target", false);
}

#[test]
fn the_edit_kernel_accepts_a_reported_wrapped_body_replacement() {
    reported_declaration_plan("replace-body", "calc", false);
}

#[test]
fn the_edit_kernel_accepts_a_reported_expression_body_replacement() {
    reported_declaration_plan("replace-body", "calc", true);
}

#[test]
fn the_edit_kernel_accepts_a_reported_go_method_replacement() {
    reported_declaration_plan("replace-body", "Calc", false);
}

#[test]
fn the_edit_kernel_accepts_a_reported_java_method_replacement() {
    reported_declaration_plan("replace-body", "CalcJava", false);
}

fn reported_declaration_plan(operation: &str, selected: &str, expression_body: bool) {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let body = operation == "replace-body";
    let go = body && selected == "Calc";
    let java = body && selected == "CalcJava";
    let (file, old, new) = if go {
        ("app.go", "{ return value + 1 }", "{ return value * 2 }")
    } else if java {
        ("App.java", "{ return value + 1; }", "{ return value * 2; }")
    } else if expression_body {
        ("app.tsx", "value + 1", "value * 2")
    } else if body {
        (
            "app.tsx",
            "{ return <span>{value + 1}</span>; }",
            "{ return <span>{value * 2}</span>; }",
        )
    } else {
        (
            "app.rs",
            "fn calc(n: i32) -> i32 { n + 1 }",
            "pub fn calc(n: i64) -> i64 { n * 2 }",
        )
    };
    let inserting = operation == "insert-declaration";
    let module = inserting && selected == "target";
    let source = if module {
        "// π\r\nmod outer {\r\n    mod target {\r\n        // } stays.\r\n    }\r\n}\r\n"
            .to_owned()
    } else if inserting {
        "// π\r\nfn other() {}\r\n// Final comment.".to_owned()
    } else if go {
        format!("// π\r\npackage main\r\ntype Counter int\r\nfunc (c *Counter) Calc(value int) int /* keep */ {old}\r\nfunc other() {{}}\r\n")
    } else if java {
        format!("// π\r\nfinal class App {{\r\n    static int CalcJava(int value) /* keep */ {old}\r\n    static int other() {{ return 0; }}\r\n}}\r\n")
    } else if expression_body {
        format!("// π\r\nconst calc = (((value: number) => /* keep */ {old}) satisfies (value: number) => number)!;\r\nconst other = () => {{ return 0; }};\r\n")
    } else if body {
        format!("// π\r\nconst calc = (((value: number) => /* keep */ {old}) satisfies (value: number) => JSX.Element)!;\r\nconst other = () => {{ return 0; }};\r\n")
    } else {
        format!("// π\r\n#[inline]\r\n{old}\r\nfn other() {{}}\r\n")
    };
    std::fs::write(workspace.join(file), &source).unwrap();
    let fragment = temp.path().join("function.txt");
    std::fs::write(&fragment, new).unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "--no-cache", "-C"])
            .arg(&workspace)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let map = run(&["project", "map", "--locals", "--fields", "handle,name"]);
    let row = map["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row[1] == selected)
        .unwrap();
    let report = run(&[
        "author",
        operation,
        row[0].as_str().unwrap(),
        "--from",
        fragment.to_str().unwrap(),
    ]);
    let key = if inserting {
        "insertion"
    } else if body {
        "body"
    } else {
        "declaration"
    };
    let span = &report[key]["before_span"];
    let replacement = if inserting {
        format!(
            "{}{}{}",
            report["insertion"]["leading_separator"].as_str().unwrap(),
            new,
            report["insertion"]["trailing_separator"].as_str().unwrap()
        )
    } else {
        new.to_owned()
    };
    let edits = [Edit::new(
        Span::new(
            span["start"].as_u64().unwrap() as usize,
            span["end"].as_u64().unwrap() as usize,
        ),
        &replacement,
        "kernel",
    )];
    let expected = if module {
        format!("// π\r\nmod outer {{\r\n    mod target {{\r\n        // }} stays.\r\n{new}\r\n    }}\r\n}}\r\n")
    } else if inserting {
        format!("{source}\r\n{new}\r\n")
    } else {
        source.replace(old, new)
    };
    assert_eq!(apply_to_string(&source, &edits).unwrap(), expected);
    kernel_accepts(&source, &edits, &expected);
}

fn kernel_windows(source: &str, edits: &[Edit]) -> Vec<(String, Vec<Edit>, String)> {
    const CONTEXT: usize = 32;
    const MAX_BYTES: usize = 256;

    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by_key(|edit| (edit.span.start, edit.span.end));
    let mut windows = Vec::new();
    let mut next = 0;
    while next < ordered.len() {
        let first = ordered[next];
        let mut start = first.span.start.saturating_sub(CONTEXT);
        while !source.is_char_boundary(start) {
            start -= 1;
        }
        let mut stop = first.span.end;
        let mut last = next + 1;
        while last < ordered.len() && ordered[last].span.end - start <= MAX_BYTES {
            stop = ordered[last].span.end;
            last += 1;
        }
        stop = (stop + CONTEXT).min(source.len());
        while !source.is_char_boundary(stop) {
            stop += 1;
        }
        let local_edits = ordered[next..last]
            .iter()
            .map(|edit| {
                Edit::new(
                    Span::new(edit.span.start - start, edit.span.end - start),
                    edit.replacement.clone(),
                    edit.reason.clone(),
                )
            })
            .collect::<Vec<_>>();
        let local_source = source[start..stop].to_string();
        let expected = apply_to_string(&local_source, &local_edits).expect("windowed edits apply");
        windows.push((local_source, local_edits, expected));
        next = last;
    }
    windows
}

fn kernel_accepts_edit_set(edits: &EditSet) {
    let checks: Vec<(String, Vec<Edit>, String)> = edits
        .iter()
        .flat_map(|(path, edits)| {
            let source = std::fs::read_to_string(path).expect("read fr source");
            kernel_windows(&source, edits)
        })
        .collect();
    assert!(checks.len() > 1, "the self plan changes multiple fr files");
    assert_eq!(
        checks
            .iter()
            .map(|(_, edits, _)| edits.len())
            .sum::<usize>(),
        edits.edit_count(),
        "every self-plan edit reaches the Lean audit"
    );
    let lean_checks: Vec<(&str, &[Edit], &str)> = checks
        .iter()
        .map(|(source, edits, expected)| (source.as_str(), edits.as_slice(), expected.as_str()))
        .collect();
    kernel_accepts_all(&lean_checks);
}

fn kernel_accepts_file_edits(source: &str, edits: &[Edit]) {
    let checks = kernel_windows(source, edits);
    assert_eq!(
        checks
            .iter()
            .map(|(_, edits, _)| edits.len())
            .sum::<usize>(),
        edits.len(),
        "every single-file self-plan edit reaches the Lean audit."
    );
    let lean_checks: Vec<(&str, &[Edit], &str)> = checks
        .iter()
        .map(|(source, edits, expected)| (source.as_str(), edits.as_slice(), expected.as_str()))
        .collect();
    kernel_accepts_all(&lean_checks);
}

#[test]
fn the_lossless_edit_kernel_agrees_with_rust() {
    build_kernel();
    let output = Command::new("lake")
        .args(["exe", "fr-edit-kernel"])
        .current_dir(root().join("kernels"))
        .output()
        .expect("Lean is installed for the kernel gate");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lean = String::from_utf8(output.stdout).expect("Lean writes text");
    let expected: Vec<String> = ["", "a", "ab", "abc", "abcd", "é", "aé", "🙂"]
        .into_iter()
        .flat_map(|source| {
            plans_for(source)
                .into_iter()
                .map(move |edits| (source, edits))
        })
        .map(|(source, edits)| {
            match apply_to_string(
                source,
                &edits
                    .into_iter()
                    .map(|(start, end, replacement)| {
                        Edit::new(Span::new(start, end), replacement, "kernel")
                    })
                    .collect::<Vec<_>>(),
            ) {
                Ok(output) => format!("ok\t{output}"),
                Err(_) => "reject".to_string(),
            }
        })
        .collect();
    let actual: Vec<&str> = lean.lines().collect();
    assert_eq!(actual.len(), expected.len(), "Lean cases:\n{lean}");
    for (at, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(actual, expected, "case {at}");
    }
}

#[test]
fn the_position_kernel_agrees_with_rust() {
    build_kernel();
    let output = Command::new("lake")
        .args(["exe", "fr-position-kernel"])
        .current_dir(root().join("kernels"))
        .output()
        .expect("Lean is installed for the kernel gate");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = Vec::new();
    for source in position_sources() {
        let index = LineIndex::new(&source);
        for offset in 0..source.len() + 3 {
            let position = index.line_col(offset, &source);
            expected.push(format!("position\t{}\t{}", position.line, position.col));
            let span = fun_refactor::edit::full_line_span(&source, offset);
            expected.push(format!("span\t{}\t{}", span.start, span.end));
        }
        for line in 0..6 {
            for col in 0..9 {
                expected.push(match index.offset(LineCol { line, col }, &source) {
                    Some(offset) => format!("offset\t{offset}"),
                    None => "none".to_string(),
                });
            }
        }
    }
    let lean = String::from_utf8(output.stdout).expect("Lean writes text");
    let actual: Vec<&str> = lean.lines().collect();
    assert_eq!(actual.len(), expected.len(), "Lean cases:\n{actual:#?}");
    for (at, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(actual, expected, "case {at}");
    }
}

#[test]
fn the_edit_kernel_accepts_a_real_rename_plan() {
    let source =
        "pub fn café() -> &'static str { \"🙂\" }\n\nfn main() { println!(\"{}\", café()); }\n";
    let workspace = tempfile::tempdir().expect("temporary workspace");
    let path = workspace.path().join("lib.rs");
    std::fs::write(&path, source).expect("write Rust source");
    let scanned = scan(workspace.path(), &ScanOptions::default()).expect("scan workspace");
    let index = Index::build_from_scan(&scanned).expect("index workspace");
    let target = index
        .find_symbols("café", None)
        .first()
        .expect("café definition")
        .id;
    let plan = rename::plan(&index, target, "bistro").expect("rename plan");
    let edits = plan.edits.edits_for(&path).expect("edits for Rust source");
    let expected = apply_to_string(source, edits).expect("Rust applies rename plan");

    kernel_accepts(source, edits, &expected);
}

#[test]
#[ignore = "the full self-audit runs after merge and on demand."]
fn the_edit_kernel_accepts_every_edit_in_a_self_rename_plan() {
    let source_root = root().join("src");
    let scanned = scan(&source_root, &ScanOptions::default()).expect("scan fr source");
    let index = Index::build_from_scan(&scanned).expect("index fr source");
    let edit_engine = source_root.join("edit.rs");
    let target = index
        .find_symbols("apply_to_string", Some(&edit_engine))
        .first()
        .expect("fr edit engine")
        .id;
    let plan = rename::plan(&index, target, "apply_kernel_to_string").expect("self rename");
    assert!(
        plan.reference_edits > 1,
        "the self rename reaches its callers"
    );

    kernel_accepts_edit_set(&plan.edits);
}

#[test]
fn the_edit_kernel_accepts_every_edit_in_a_self_signature_plan() {
    let source_root = root().join("src");
    let scanned = scan(&source_root, &ScanOptions::default()).expect("scan fr source");
    let index = Index::build_from_scan(&scanned).expect("index fr source");
    let edit_engine = source_root.join("edit.rs");
    let target = index
        .find_symbols("line_indent", Some(&edit_engine))
        .first()
        .expect("fr line indentation helper")
        .id;
    let plan = signature::change(
        &index,
        target,
        signature::Change::Add {
            at: 2,
            declaration: "kernel_marker: bool".to_string(),
            argument: "false".to_string(),
        },
    )
    .expect("self signature change");
    assert!(
        plan.call_sites > 10,
        "the self signature plan reaches its callers."
    );
    kernel_accepts_edit_set(&plan.edits);
}

#[test]
#[ignore = "the full self-audit runs after merge and on demand."]
fn the_edit_kernel_accepts_every_edit_in_a_self_move_plan() {
    let source_root = root().join("src");
    let scanned = scan(&source_root, &ScanOptions::default()).expect("scan fr source");
    let index = Index::build_from_scan(&scanned).expect("index fr source");
    let edit_engine = source_root.join("edit.rs");
    let target = index
        .find_symbols("indent_unit", Some(&edit_engine))
        .first()
        .expect("fr indentation helper")
        .id;
    let plan =
        move_symbol::to_file(&index, target, &source_root.join("span.rs")).expect("self move plan");
    assert!(
        plan.edits.file_count() > 3,
        "the self move plan changes declarations, imports, and callers"
    );
    kernel_accepts_edit_set(&plan.edits);
}

#[test]
fn the_edit_kernel_accepts_a_self_extract_plan() {
    let source_root = root().join("src");
    let scanned = scan(&source_root, &ScanOptions::default()).expect("scan fr source");
    let index = Index::build_from_scan(&scanned).expect("index fr source");
    let edit_engine = source_root.join("edit.rs");
    let source = std::fs::read_to_string(&edit_engine).expect("read fr edit engine");
    let start = source
        .find("source.to_string()")
        .expect("source copy expression");
    let plan = extract::variable(
        &index,
        &edit_engine,
        Span::new(start, start + "source.to_string()".len()),
        "kernel_source_copy",
        false,
    )
    .expect("self extraction plan");
    assert_eq!(plan.occurrences, 1, "the selected expression changes once");
    let edits = plan
        .edits
        .edits_for(&edit_engine)
        .expect("self extraction edits");
    kernel_accepts_file_edits(&source, edits);
}

#[test]
fn the_edit_kernel_accepts_a_self_inline_plan() {
    let source_root = root().join("src");
    let scanned = scan(&source_root, &ScanOptions::default()).expect("scan fr source");
    let index = Index::build_from_scan(&scanned).expect("index fr source");
    let position_engine = source_root.join("span.rs");
    let source = std::fs::read_to_string(&position_engine).expect("read fr position engine");
    let binding = source
        .find("line_start = self.line_starts[line]")
        .expect("line start binding");
    let target = index
        .definition_at(&position_engine, binding)
        .expect("line start definition")
        .id;
    let plan = inline::variable(&index, target).expect("self inline plan");
    assert_eq!(plan.use_sites, 2, "the self inline rewrites both uses");
    let edits = plan
        .edits
        .edits_for(&position_engine)
        .expect("self inline edits");
    kernel_accepts_file_edits(&source, edits);
}

#[test]
fn the_edit_kernel_accepts_a_self_inline_call_plan() {
    let source_root = root().join("src");
    let scanned = scan(&source_root, &ScanOptions::default()).expect("scan fr source");
    let index = Index::build_from_scan(&scanned).expect("index fr source");
    let position_engine = source_root.join("span.rs");
    let source = std::fs::read_to_string(&position_engine).expect("read fr position engine");
    let call = source.find("self.line_end(line)").expect("line end call") + "self.".len();
    let plan = inline::call(&index, &position_engine, call).expect("self inline call plan");
    assert!(
        plan.expansion.contains("match self.line_starts"),
        "the self inline call substitutes the callee expression."
    );
    fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
        .expect("the self inline call reparses");
    let edits = plan
        .edits
        .edits_for(&position_engine)
        .expect("self inline call edits");
    kernel_accepts_file_edits(&source, edits);
}

#[test]
fn a_self_imports_plan_reparses() {
    let source_root = root().join("src");
    let scanned = scan(&source_root, &ScanOptions::default()).expect("scan fr source");
    let index = Index::build_from_scan(&scanned).expect("index fr source");
    let command_surface = source_root.join("cli.rs");
    let plan = imports::plan(&index, &command_surface).expect("self imports plan");
    assert_eq!(
        plan.removed.len(),
        0,
        "the self plan keeps the external Context trait import."
    );
    assert_eq!(
        plan.sorted_blocks, 1,
        "the self plan reorders its import block"
    );
    fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
        .expect("the self imports plan reparses");
}

#[test]
fn history_snapshot_checks_match_lean_for_existence_content_and_modes() {
    use fun_refactor::history::{matches_snapshot, Snapshot, SnapshotKind};
    build_kernel();
    let output = Command::new("lake")
        .args(["exe", "fr-history-kernel"])
        .current_dir(root().join("kernels"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let samples = [
        None,
        Some(Snapshot {
            content: String::new(),
            mode: 0o600,
            kind: fun_refactor::history::SnapshotKind::Regular,
        }),
        Some(Snapshot {
            content: "λ\n".to_string(),
            mode: 0o600,
            kind: fun_refactor::history::SnapshotKind::Regular,
        }),
        Some(Snapshot {
            content: "λ\n".to_string(),
            mode: 0o751,
            kind: fun_refactor::history::SnapshotKind::Regular,
        }),
        Some(Snapshot {
            content: "名".to_string(),
            mode: 0o644,
            kind: fun_refactor::history::SnapshotKind::Regular,
        }),
        Some(Snapshot {
            content: "target".to_string(),
            mode: 0,
            kind: SnapshotKind::Symlink,
        }),
    ];
    let mut expected = Vec::new();
    for current in &samples {
        for before in &samples {
            for after in &samples {
                for recovery in [false, true] {
                    expected.push(matches_snapshot(current, before, after, recovery).to_string());
                }
            }
        }
    }
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn patch_modes_match_lean_across_permission_bits_and_u32_boundaries() {
    use fun_refactor::history::{git_mode, git_mode_change_supported};
    build_kernel();
    let output = Command::new("lake")
        .args(["exe", "fr-history-kernel", "patch-modes"])
        .current_dir(root().join("kernels"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let modes = (0..4096u32)
        .chain((0..32).map(|bit| 1u32 << bit))
        .chain([u32::MAX]);
    let mut expected = Vec::new();
    for mode in modes {
        expected.push(git_mode(mode).to_string());
        for mask in [0, 1, 8, 9, 64, 65, 72, 73, 0o222, u32::MAX] {
            expected.push(git_mode_change_supported(mode, mode ^ mask).to_string());
        }
    }
    assert_eq!(expected.len(), 45_419);
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    assert_eq!(observed.len(), expected.len());
    for (case, (observed, expected)) in observed.iter().zip(&expected).enumerate() {
        assert_eq!(observed, expected, "mode case {case}");
    }
}

#[test]
fn snapshot_git_modes_match_lean_for_regular_files_and_symlinks() {
    use fun_refactor::history::git_snapshot_mode;
    build_kernel();
    let output = Command::new("lake")
        .args(["exe", "fr-history-kernel", "snapshot-modes"])
        .current_dir(root().join("kernels"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = Vec::new();
    for mode in (0..4096u32)
        .chain((0..32).map(|bit| 1u32 << bit))
        .chain([u32::MAX])
    {
        for symlink in [false, true] {
            expected.push(git_snapshot_mode(symlink, mode).to_string());
        }
    }
    assert_eq!(expected.len(), 8_258);
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    assert_eq!(observed.len(), expected.len());
    for (case, (observed, expected)) in observed.iter().zip(&expected).enumerate() {
        assert_eq!(observed, expected, "snapshot mode case {case}");
    }
}

#[test]
fn workflow_stage_policy_matches_lean_for_every_state_and_stage() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-history-kernel"))
        .arg("workflow-stages")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    let mut expected = Vec::new();
    for applied in [false, true] {
        for stage in 0..6 {
            expected.push(fun_refactor::workflow::workflow_stage_state(applied, stage).to_string());
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn task_change_review_policy_matches_lean_for_every_boolean_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-history-kernel"))
        .arg("task-change-modes")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    let mut expected = Vec::new();
    for complete in [false, true] {
        for write in [false, true] {
            for supplied in [false, true] {
                for matches in [false, true] {
                    expected.push(
                        fun_refactor::project::task_change::task_change_mode(
                            complete, write, supplied, matches,
                        )
                        .to_string(),
                    );
                }
            }
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn browser_history_transition_policy_matches_lean_exhaustively() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-history-kernel"))
        .arg("memory-transitions")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    let mut expected = Vec::new();
    for status in 0..4 {
        for action in 0..3 {
            for at_top in [false, true] {
                expected.push(
                    fun_refactor::transaction_kernel::memory_transition_allowed(
                        status, action, at_top,
                    )
                    .to_string(),
                );
            }
        }
    }
    assert_eq!(observed, expected);
}

#[test]
fn owner_executable_settings_match_lean_across_permission_bits_and_u32_boundaries() {
    use fun_refactor::history::owner_executable_mode;
    build_kernel();
    let output = Command::new("lake")
        .args(["exe", "fr-history-kernel", "owner-executable"])
        .current_dir(root().join("kernels"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = Vec::new();
    for mode in (0..4096u32)
        .chain((0..32).map(|bit| 1u32 << bit))
        .chain([u32::MAX])
    {
        for executable in [false, true] {
            expected.push(owner_executable_mode(mode, executable).to_string());
        }
    }
    assert_eq!(expected.len(), 8_258);
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    assert_eq!(observed.len(), expected.len());
    for (case, (observed, expected)) in observed.iter().zip(&expected).enumerate() {
        assert_eq!(observed, expected, "executable case {case}");
    }
}

#[test]
fn patch_basis_matches_lean_for_existence_contents_and_projected_permissions() {
    use fun_refactor::history::{matches_patch_basis, matches_snapshot, Snapshot, SnapshotKind};
    build_kernel();
    let output = Command::new("lake")
        .args(["exe", "fr-history-kernel", "patch-basis"])
        .current_dir(root().join("kernels"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut samples = vec![None];
    for content in ["", "λ\n", "名", "a\0b"] {
        for mode in [0, 1, 64, 73, 0o600, 0o644, 0o700, 0o755, 0o7777, u32::MAX] {
            samples.push(Some(Snapshot {
                content: content.into(),
                mode,
                kind: fun_refactor::history::SnapshotKind::Regular,
            }));
        }
    }
    samples.push(Some(Snapshot {
        content: "target".into(),
        mode: 0,
        kind: SnapshotKind::Symlink,
    }));
    samples.push(Some(Snapshot {
        content: "other".into(),
        mode: 0,
        kind: SnapshotKind::Symlink,
    }));
    let mut expected = Vec::new();
    let mut permissions_only = 0;
    for actual in &samples {
        for basis in &samples {
            let matches = matches_patch_basis(actual, basis);
            let exact = matches_snapshot(actual, basis, &None, false);
            assert!(!exact || matches);
            permissions_only += usize::from(matches && !exact);
            expected.push(matches.to_string());
        }
    }
    assert!(permissions_only > 0);
    assert_eq!(expected.len(), 1_849);
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    assert_eq!(observed.len(), expected.len());
    for (case, (observed, expected)) in observed.iter().zip(&expected).enumerate() {
        assert_eq!(observed, expected, "basis case {case}");
    }
}

#[test]
fn project_page_lengths_match_lean_including_integer_limits() {
    build_kernel();
    let output = Command::new("lake")
        .args(["exe", "fr-project-kernel"])
        .current_dir(root().join("kernels"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let samples: [u64; 12] = [
        0,
        1,
        2,
        3,
        4,
        79,
        80,
        499,
        500,
        65536,
        u32::MAX.into(),
        u64::MAX,
    ];
    let actual = String::from_utf8(output.stdout).unwrap();
    let actual = actual
        .lines()
        .map(|line| line.parse::<u64>().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(actual.len(), samples.len().pow(3));
    let mut index = 0;
    for total in samples {
        for start in samples {
            for limit in samples {
                if let (Ok(total), Ok(start), Ok(limit)) = (
                    usize::try_from(total),
                    usize::try_from(start),
                    usize::try_from(limit),
                ) {
                    assert_eq!(
                        fun_refactor::project::page_length(total, start, limit) as u64,
                        actual[index]
                    );
                }
                index += 1;
            }
        }
    }
}

#[test]
fn project_batch_budgets_match_lean_including_integer_limits() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("batch-budgets")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let samples: [u64; 12] = [
        0,
        1,
        2,
        3,
        4,
        79,
        80,
        499,
        500,
        65536,
        u32::MAX.into(),
        u64::MAX,
    ];
    let actual = String::from_utf8(output.stdout).unwrap();
    let mut actual = actual.lines().map(|line| line.parse::<bool>().unwrap());
    let mut checked = 0usize;
    for used in samples {
        for next in samples {
            for budget in samples {
                let observed = actual.next().expect("one Lean result per budget case");
                if let (Ok(used), Ok(next), Ok(budget)) = (
                    usize::try_from(used),
                    usize::try_from(next),
                    usize::try_from(budget),
                ) {
                    let expected = fun_refactor::project::batch_section_fits(used, next, budget);
                    assert_eq!(
                        observed, expected,
                        "used {used}, next {next}, budget {budget}"
                    );
                    if expected {
                        assert!(used.checked_add(next).is_some_and(|sum| sum <= budget));
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(actual.next().is_none());
    assert_eq!(checked, if usize::BITS == 64 { 1728 } else { 1331 });
}

#[test]
fn project_task_authoring_targets_match_lean_exhaustively() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("task-author-targets")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let actual = String::from_utf8(output.stdout).unwrap();
    let mut actual = actual.lines().map(|line| line.parse::<bool>().unwrap());
    let mut checked = 0usize;
    for operation in 0..7 {
        for language in 0..22 {
            for target in 0..9 {
                assert_eq!(
                    actual.next(),
                    Some(fun_refactor::project::task_author_target_candidate(
                        operation, language, target
                    )),
                    "operation {operation}, language {language}, target {target}"
                );
                checked += 1;
            }
        }
    }
    assert!(actual.next().is_none());
    assert_eq!(checked, 1_386);
}

#[test]
fn semantic_section_budget_matches_lean_on_boundary_cases() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("semantic-sections")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let actual = String::from_utf8(output.stdout).unwrap();
    let mut actual = actual.lines().map(|line| line.parse::<bool>().unwrap());
    let samples = [
        0usize,
        1,
        2,
        3,
        4,
        79,
        80,
        499,
        500,
        65_536,
        u32::MAX as usize,
        usize::MAX,
    ];
    for required in samples {
        for budget in samples {
            assert_eq!(
                actual.next(),
                Some(fun_refactor::project::semantic_section_fits(
                    required, budget
                )),
                "required {required}, budget {budget}"
            );
        }
    }
    assert!(actual.next().is_none());
}

#[test]
fn semantic_body_admission_matches_lean_exhaustively() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("semantic-author-admission")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let actual = String::from_utf8(output.stdout).unwrap();
    let mut actual = actual.lines().map(|line| line.parse::<bool>().unwrap());
    for schema in [false, true] {
        for target in [false, true] {
            for source_free in [false, true] {
                for bounded in [false, true] {
                    assert_eq!(
                        actual.next(),
                        Some(fun_refactor::project::author::semantic_body_admitted(
                            schema,
                            target,
                            source_free,
                            bounded,
                        ))
                    );
                }
            }
        }
    }
    assert!(actual.next().is_none());
}

#[test]
fn framework_boundary_policies_match_lean_over_the_bounded_domains() {
    use fun_refactor::project::framework_kernel::{
        component_hooks_compatible, configuration_visibility, fastapi_body_parameter_automatic,
        fastapi_prefix_supported, fastapi_registration_automatic, framework_emitted,
        framework_migration_supported, framework_omitted, middleware_request_order,
        migration_cutover_automatic, migration_dependency_edit_automatic, migration_disposition,
        migration_schema_agreement, nextjs_body_validation_automatic,
        nextjs_registration_automatic, service_redaction_flags, service_target_kind,
    };

    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("framework-boundaries")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout).unwrap();
    let actual = actual.lines().collect::<Vec<_>>();
    let samples = [0, 1, 2, 63, 64, 65, 128, 512, 65536];
    let mut expected = Vec::new();
    for total in samples {
        for limit in samples {
            expected.push(framework_emitted(total, limit).to_string());
            expected.push(framework_omitted(total, limit).to_string());
        }
    }
    for total in samples {
        for declaration_index in samples {
            expected.push(middleware_request_order(total, declaration_index).to_string());
        }
    }
    for client in [false, true] {
        for runtime_hooks in [0, 1, 2, 65536] {
            expected.push(component_hooks_compatible(client, runtime_hooks).to_string());
        }
    }
    for nextjs in [false, true] {
        for public_name in [false, true] {
            expected.push(configuration_visibility(nextjs, public_name).to_string());
        }
    }
    for absolute_http in [false, true] {
        for root_relative in [false, true] {
            expected.push(service_target_kind(absolute_http, root_relative).to_string());
        }
    }
    for query_or_fragment in [false, true] {
        for credentials in [false, true] {
            expected.push(service_redaction_flags(query_or_fragment, credentials).to_string());
        }
    }
    for empty in [false, true] {
        for starts_slash in [false, true] {
            for ends_slash in [false, true] {
                expected
                    .push(fastapi_prefix_supported(empty, starts_slash, ends_slash).to_string());
            }
        }
    }
    for source_fastapi in [false, true] {
        for target_fastapi in [false, true] {
            expected
                .push(framework_migration_supported(source_fastapi, target_fastapi).to_string());
        }
    }
    for gap in [false, true] {
        for automatic_kind in [false, true] {
            expected.push(migration_disposition(gap, automatic_kind).to_string());
        }
    }
    let schema_samples = [
        vec![],
        vec!["a".to_owned()],
        vec!["a".to_owned(), "b".to_owned()],
        vec!["b".to_owned(), "a".to_owned()],
        vec!["a".to_owned(), "a".to_owned()],
    ];
    for expected_shapes in &schema_samples {
        for generated_shapes in &schema_samples {
            expected
                .push(migration_schema_agreement(expected_shapes, generated_shapes).to_string());
        }
    }
    for declares_next in [false, true] {
        for app_router_path in [false, true] {
            expected
                .push(nextjs_registration_automatic(declares_next, app_router_path).to_string());
        }
    }
    for candidate_count in [0, 1, 2, 65536] {
        for path_collision in [false, true] {
            for query_collision in [false, true] {
                expected.push(
                    fastapi_body_parameter_automatic(
                        candidate_count,
                        path_collision,
                        query_collision,
                    )
                    .to_string(),
                );
            }
        }
    }
    for candidate_count in [0, 1, 2, 65536] {
        for supported_shape in [false, true] {
            expected.push(
                nextjs_body_validation_automatic(candidate_count, supported_shape).to_string(),
            );
        }
    }
    for explicit_target in [false, true] {
        for application_binding in [false, true] {
            for endpoint_conflict in [false, true] {
                expected.push(
                    fastapi_registration_automatic(
                        explicit_target,
                        application_binding,
                        endpoint_conflict,
                    )
                    .to_string(),
                );
            }
        }
    }
    for explicit_cutover in [false, true] {
        for registration_automatic in [false, true] {
            for external_references in [false, true] {
                expected.push(
                    migration_cutover_automatic(
                        explicit_cutover,
                        registration_automatic,
                        external_references,
                    )
                    .to_string(),
                );
            }
        }
    }
    for pep621_manifest in [false, true] {
        for owns_destination in [false, true] {
            for dependencies_array in [false, true] {
                for requirements_cover_missing in [false, true] {
                    expected.push(
                        migration_dependency_edit_automatic(
                            pep621_manifest,
                            owns_destination,
                            dependencies_array,
                            requirements_cover_missing,
                        )
                        .to_string(),
                    );
                }
            }
        }
    }
    for executed in [false, true] {
        for commands_passed in [false, true] {
            for configuration_stable in [false, true] {
                for source_snapshot_stable in [false, true] {
                    expected.push(
                        fun_refactor::checks::check_evidence_acceptable(
                            executed,
                            commands_passed,
                            configuration_stable,
                            source_snapshot_stable,
                        )
                        .to_string(),
                    );
                }
            }
        }
    }
    for requirement_present in [false, true] {
        for configuration_matches in [false, true] {
            for check_names_match in [false, true] {
                expected.push(
                    fun_refactor::checks::check_requirement_satisfied(
                        requirement_present,
                        configuration_matches,
                        check_names_match,
                    )
                    .to_string(),
                );
            }
        }
    }
    assert_eq!(actual, expected);
}

fn source_samples() -> Vec<String> {
    let mut sources = vec![String::new()];
    let mut words = sources.clone();
    for _ in 0..3 {
        words = words
            .iter()
            .flat_map(|stem| ["a", "é", "名", "🙂"].map(|suffix| format!("{stem}{suffix}")))
            .collect();
        sources.extend(words.clone());
    }
    sources.extend(
        [
            "\0\r\n",
            "\u{7f}\u{80}\u{7ff}\u{800}\u{ffff}\u{10000}\u{10ffff}",
            "é",
            "\u{feff}",
            "\t\\\"",
        ]
        .map(str::to_owned),
    );
    sources
}

fn source_budgets() -> Vec<u64> {
    (0..17).chain([65536, u32::MAX.into(), u64::MAX]).collect()
}

fn scalar_slice_length(text: &str, offset: usize, budget: usize) -> Option<usize> {
    let tail = text.get(offset..)?;
    let mut length = 0;
    for character in tail.chars() {
        let width = character.len_utf8();
        if width > budget - length {
            break;
        }
        length += width;
    }
    Some(length)
}

#[test]
fn source_slice_lengths_match_lean_and_scalar_oracle() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("source-slices")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut actual = stdout.lines();
    let mut count = 0;
    for text in source_samples() {
        for offset in (0..text.len() as u64 + 2).chain([u32::MAX.into(), u64::MAX]) {
            for budget in source_budgets() {
                let line = actual.next().expect("one Lean result per case");
                if let (Ok(offset), Ok(budget)) = (usize::try_from(offset), usize::try_from(budget))
                {
                    let expected =
                        fun_refactor::project::source_slice_length(&text, offset, budget);
                    let lean = (line != "none").then(|| line.parse::<usize>().unwrap());
                    assert_eq!(lean, expected, "{text:?}, offset {offset}, budget {budget}");
                    assert_eq!(expected, scalar_slice_length(&text, offset, budget));
                    if let Some(length) = expected {
                        let end = offset.checked_add(length).unwrap();
                        assert!(length <= budget && text.is_char_boundary(end));
                        assert_eq!(
                            format!("{}{}{}", &text[..offset], &text[offset..end], &text[end..]),
                            text
                        );
                    }
                    count += 1;
                }
            }
        }
    }
    assert!(actual.next().is_none());
    assert_eq!(count, if usize::BITS == 64 { 19_220 } else { 16_549 });
}

#[test]
fn source_page_allocations_match_lean_and_share_one_budget() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("source-pages")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut actual = stdout.lines();
    let mut pages = vec![Vec::new()];
    let mut words = pages.clone();
    for _ in 0..3 {
        words = words
            .iter()
            .flat_map(|stem| {
                ["", "a", "é", "名", "🙂", "a🙂é"].map(|text| {
                    let mut page = stem.clone();
                    page.push(text);
                    page
                })
            })
            .collect();
        pages.extend(words.clone());
    }
    assert_eq!(pages.len(), 259);
    for texts in pages {
        for budget in source_budgets() {
            let line = actual.next().expect("one Lean result per page");
            let Ok(budget) = usize::try_from(budget) else {
                continue;
            };
            let mut remaining = budget;
            let expected: Vec<_> = texts
                .iter()
                .map(|text| {
                    let length =
                        fun_refactor::project::source_slice_length(text, 0, remaining).unwrap();
                    assert_eq!(Some(length), scalar_slice_length(text, 0, remaining));
                    remaining = remaining.checked_sub(length).unwrap();
                    length
                })
                .collect();
            let lean: Vec<usize> = serde_json::from_str(line).unwrap();
            assert_eq!(lean, expected, "{texts:?}, budget {budget}");
            assert_eq!(lean.len(), texts.len());
            assert_eq!(
                lean.iter().sum::<usize>().checked_add(remaining),
                Some(budget)
            );
        }
    }
    assert!(actual.next().is_none());
}

#[test]
fn find_source_pages_agree_with_lean_on_selected_source() {
    build_kernel();
    let dir = tempfile::tempdir().unwrap();
    let original = "def café():\n    return 'λ🙂'\n\nclass Other:\n    def café(self):\n        return '世界'\n\nclass Third:\n    def café(self):\n        return 'é'\n";
    std::fs::write(dir.path().join("app.py"), original).unwrap();
    let query = |budget: usize| -> serde_json::Value {
        let output = Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "--no-cache", "-C"])
            .arg(dir.path())
            .args([
                "project",
                "find",
                "café",
                "--source",
                "--bytes",
                &budget.to_string(),
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        serde_json::from_slice(&output.stdout).unwrap()
    };
    let full = query(65536);
    let source_column = full["columns"]
        .as_array()
        .unwrap()
        .iter()
        .position(|field| field == "source")
        .unwrap();
    let texts: Vec<_> = full["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let source = &row[source_column];
            let start = source["span"]["start"].as_u64().unwrap() as usize;
            let end = source["span"]["end"].as_u64().unwrap() as usize;
            let text = &original[start..end];
            assert_eq!(source["text"], text);
            assert_eq!(source["next_offset"], serde_json::Value::Null);
            text
        })
        .collect();
    assert_eq!(texts.len(), 3);
    for budget in [4, 5, 6, 7, 8, 9, 12, 16, 32, 64, 65536] {
        let report = query(budget);
        let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
            .args(["source-page", &budget.to_string()])
            .args(&texts)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let lean: Vec<usize> = serde_json::from_slice(&output.stdout).unwrap();
        let rows = report["rows"].as_array().unwrap();
        assert_eq!(rows.len(), texts.len());
        assert_eq!(lean.len(), texts.len());
        for ((row, text), length) in rows.iter().zip(&texts).zip(&lean) {
            let source = &row[source_column];
            assert_eq!(source["text"], text[..*length]);
            assert_eq!(source["returned_bytes"], *length);
            assert_eq!(
                source["next_offset"],
                serde_json::json!((*length < text.len()).then_some(length))
            );
        }
        assert_eq!(
            report["source_budget"]["returned_bytes"],
            lean.iter().sum::<usize>()
        );
    }
    assert_eq!(
        std::fs::read_to_string(dir.path().join("app.py")).unwrap(),
        original
    );
}

#[test]
fn workspace_pattern_matcher_agrees_with_lean_on_component_sequences() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("patterns")
        .output()
        .unwrap();
    assert!(output.status.success());
    let values: Vec<_> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect();
    let alphabet = ["a", "b", "*", "λ", "", "a/b"];
    let mut paths: Vec<Vec<String>> = vec![vec![]];
    let mut words = paths.clone();
    for _ in 0..3 {
        words = words
            .iter()
            .flat_map(|prefix| {
                alphabet.iter().map(move |part| {
                    let mut path = prefix.clone();
                    path.push((*part).to_owned());
                    path
                })
            })
            .collect();
        paths.extend(words.clone());
    }
    assert_eq!(values.len(), 67_081);
    let mut values = values.into_iter();
    for pattern in &paths {
        for path in &paths {
            assert_eq!(
                values.next().unwrap(),
                fun_refactor::project::workspace_pattern_matches(pattern, path),
                "{pattern:?} {path:?}"
            );
        }
    }
}

#[test]
fn test_path_confidence_matches_lean_for_all_tier_sequences_through_six_edges() {
    use fun_refactor::model::Confidence::{Exact, FieldBased, ImportQualified, NameOnly};
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("confidence")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual: Vec<usize> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse().unwrap())
        .collect();
    let tiers = [Exact, ImportQualified, FieldBased, NameOnly];
    let mut paths = vec![vec![]];
    let mut words = paths.clone();
    for _ in 0..6 {
        words = words
            .iter()
            .flat_map(|path| {
                tiers.iter().map(move |tier| {
                    let mut next = path.clone();
                    next.push(*tier);
                    next
                })
            })
            .collect();
        paths.extend(words.clone());
    }
    assert_eq!(actual.len(), 5461);
    assert_eq!(
        actual,
        paths
            .iter()
            .map(|path| {
                tiers
                    .iter()
                    .position(|tier| *tier == fun_refactor::project::path_confidence(path))
                    .unwrap()
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn workspace_membership_rounds_match_lean_and_independent_reachability() {
    use fun_refactor::project::workspace_membership_step;
    use std::collections::{BTreeSet, VecDeque};

    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("membership")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout).unwrap();
    let mut values = actual
        .lines()
        .map(|line| serde_json::from_str::<Vec<u64>>(line).unwrap());
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("closure")
        .output()
        .unwrap();
    assert!(output.status.success());
    let closures = String::from_utf8(output.stdout).unwrap();
    let mut closures = closures
        .lines()
        .map(|line| serde_json::from_str::<Vec<u64>>(line).unwrap());
    let mut cases = 0;
    for size in 0..4 {
        for seed_mask in 0..(1 << size) {
            let seeds: Vec<_> = (0..size)
                .filter(|node| seed_mask & (1 << node) != 0)
                .collect();
            for edge_mask in 0..(1 << (size * size)) {
                let edges: Vec<_> = (0..size)
                    .flat_map(|source| (0..size).map(move |target| (source, target)))
                    .enumerate()
                    .filter_map(|(index, edge)| (edge_mask & (1 << index) != 0).then_some(edge))
                    .collect();
                let mut reachable: BTreeSet<_> = seeds.iter().copied().collect();
                let mut pending: VecDeque<_> = seeds.iter().copied().collect();
                while let Some(source) = pending.pop_front() {
                    for &(from, target) in &edges {
                        if from == source && reachable.insert(target) {
                            pending.push_back(target);
                        }
                    }
                }
                let mut current = seeds.clone();
                for round in 0..size + 2 {
                    assert_eq!(
                        current.iter().map(|v| *v as u64).collect::<Vec<_>>(),
                        values.next().unwrap(),
                        "size={size}, seeds={seed_mask}, edges={edge_mask}, round={round}"
                    );
                    cases += 1;
                    let next = workspace_membership_step(&current, &edges);
                    assert!(current.iter().all(|node| next.contains(node)));
                    if round >= size {
                        assert_eq!(current, next);
                        assert_eq!(current, reachable.iter().copied().collect::<Vec<_>>());
                    }
                    current = next;
                }
                assert_eq!(
                    current.iter().map(|v| *v as u64).collect::<Vec<_>>(),
                    closures.next().unwrap(),
                    "closure: size={size}, seeds={seed_mask}, edges={edge_mask}"
                );
            }
        }
    }
    assert_eq!(cases, 20_750);
    let seeds = [u32::MAX as u64, 7, 7];
    let edges = [(7, u64::MAX), (7, 3), (7, 3), (3, 7), (99, 2)];
    let mut current = vec![7, u32::MAX as u64];
    for round in 0..4 {
        let expected = values.next().unwrap();
        if usize::try_from(u64::MAX).is_ok() {
            let members: Vec<_> = if round == 0 {
                seeds.to_vec()
            } else {
                current.clone()
            }
            .into_iter()
            .map(|v| usize::try_from(v).unwrap())
            .collect();
            let edges: Vec<_> = edges
                .iter()
                .map(|&(s, t)| (usize::try_from(s).unwrap(), usize::try_from(t).unwrap()))
                .collect();
            assert_eq!(current, expected);
            current = workspace_membership_step(&members, &edges)
                .into_iter()
                .map(|v| v as u64)
                .collect();
        }
    }
    assert!(values.next().is_none());
    let expected = closures.next().unwrap();
    if usize::try_from(u64::MAX).is_ok() {
        assert_eq!(current, expected);
    }
    for size in [4, 16, 64] {
        let edges: Vec<_> = (0..size).map(|node| (node, node + 1)).collect();
        let mut members = vec![0];
        let mut changing_rounds = 0;
        loop {
            let next = workspace_membership_step(&members, &edges);
            if next == members {
                break;
            }
            changing_rounds += 1;
            assert!(changing_rounds <= edges.len());
            members = next;
        }
        assert_eq!(changing_rounds, size);
        assert_eq!(members, (0..=size).collect::<Vec<_>>());
        assert_eq!(
            members.iter().map(|v| *v as u64).collect::<Vec<_>>(),
            closures.next().unwrap()
        );
    }
    assert!(closures.next().is_none());
}

#[test]
fn git_line_ranges_match_lean_including_integer_limits() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("line-ranges")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout).unwrap();
    let actual = actual
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let samples: [u64; 12] = [
        0,
        1,
        2,
        3,
        4,
        79,
        80,
        499,
        500,
        65536,
        u32::MAX.into(),
        u64::MAX,
    ];
    assert_eq!(actual.len(), samples.len().pow(3));
    let mut index = 0;
    for start in samples {
        for end in samples {
            for line in samples {
                if let (Ok(start), Ok(end), Ok(line)) = (
                    usize::try_from(start),
                    usize::try_from(end),
                    usize::try_from(line),
                ) {
                    assert_eq!(
                        fun_refactor::git::line_in_range(start, end, line),
                        actual[index]
                    );
                }
                index += 1;
            }
        }
    }
}

#[test]
fn git_call_selection_matches_lean_for_every_boolean_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("call-selection")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout).unwrap();
    let actual = actual
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for incoming in [false, true] {
        for outgoing in [false, true] {
            for include_incoming in [false, true] {
                for include_outgoing in [false, true] {
                    expected.push(fun_refactor::git::call_in_selection(
                        incoming,
                        outgoing,
                        include_incoming,
                        include_outgoing,
                    ));
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn staging_transition_matches_lean_for_every_boolean_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("staging-transition")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout).unwrap();
    let actual = actual
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for before in [false, true] {
        for after in [false, true] {
            for recovery in [false, true] {
                expected.push(fun_refactor::git::staging_transition_allowed(
                    before, after, recovery,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn staging_record_compaction_matches_lean_for_every_boolean_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("staging-record-compaction")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for detailed in [false, true] {
        for pending in [false, true] {
            for retained in [false, true] {
                expected.push(fun_refactor::git::staging_record_compactable(
                    detailed, pending, retained,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn staging_crash_state_matches_lean_for_every_boolean_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("staging-crash-state")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for pending in [false, true] {
        for index_lock in [false, true] {
            for preparation in [false, true] {
                expected.push(fun_refactor::git::staging_crash_state_requires_review(
                    pending,
                    index_lock,
                    preparation,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn staging_index_entry_policy_matches_lean_for_every_boolean_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("staging-index-entry")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for stage_zero in [false, true] {
        for intent_to_add in [false, true] {
            for assume_unchanged in [false, true] {
                for skip_worktree in [false, true] {
                    expected.push(fun_refactor::git::staging_index_entry_replayable(
                        stage_zero,
                        intent_to_add,
                        assume_unchanged,
                        skip_worktree,
                    ));
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn commit_basis_matches_lean_for_branch_and_parent_changes() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("commit-basis")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout).unwrap();
    let actual = actual
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for branch in ["refs/heads/main", "refs/heads/other", "名"] {
        for observed in ["refs/heads/main", "refs/heads/other", "名"] {
            for parent in [None, Some("aa".to_owned()), Some("bb".to_owned())] {
                for observed_parent in [None, Some("aa".to_owned()), Some("bb".to_owned())] {
                    expected.push(fun_refactor::git::commit_basis_matches(
                        branch,
                        observed,
                        &parent,
                        &observed_parent,
                    ));
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_budget_matches_lean_at_limits_and_machine_boundaries() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-budget")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for files in [0, 1, 19999, 20000, 20001, usize::MAX] {
        for bytes in [0, 268435455, 268435456, 268435457, usize::MAX] {
            for blob_bytes in [0, 33554431, 33554432, 33554433, usize::MAX] {
                expected.push(fun_refactor::git::worktree_budget_allows(
                    files, bytes, blob_bytes,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_recovery_matches_lean_for_all_file_states() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-recovery")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for present in [false, true] {
        for bytes_match in [false, true] {
            for mode_matches in [false, true] {
                expected.push(fun_refactor::git::worktree_recovery_file_allowed(
                    present,
                    bytes_match,
                    mode_matches,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_configuration_matches_lean_for_every_boolean_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-configuration")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for reviewed_mode in [false, true] {
        for observed_mode in [false, true] {
            for config_present in [false, true] {
                for config_regular in [false, true] {
                    expected.push(fun_refactor::git::worktree_configuration_allowed(
                        reviewed_mode,
                        observed_mode,
                        config_present,
                        config_regular,
                    ));
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_prepared_recovery_policy_matches_lean_for_every_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-prepared-recovery")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for receipt_present in [false, true] {
        for preparation_matches in [false, true] {
            for registration_matches in [false, true] {
                expected.push(fun_refactor::git::worktree_prepared_recovery_allowed(
                    receipt_present,
                    preparation_matches,
                    registration_matches,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_entry_mode_policy_matches_lean_for_every_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-entry-mode")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for regular in [false, true] {
        for executable in [false, true] {
            for symlink in [false, true] {
                for object_is_blob in [false, true] {
                    expected.push(fun_refactor::git::worktree_entry_mode_allowed(
                        regular,
                        executable,
                        symlink,
                        object_is_blob,
                    ));
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_removal_matches_lean_for_all_file_states() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-removal")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for identity_matches in [false, true] {
        for bytes_match in [false, true] {
            for mode_matches in [false, true] {
                expected.push(fun_refactor::git::worktree_removal_file_allowed(
                    identity_matches,
                    bytes_match,
                    mode_matches,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_removal_resumption_matches_lean_for_all_file_states() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-removal-resume")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for present in [false, true] {
        for identity_matches in [false, true] {
            for bytes_match in [false, true] {
                for mode_matches in [false, true] {
                    expected.push(fun_refactor::git::worktree_removal_resume_allowed(
                        present,
                        identity_matches,
                        bytes_match,
                        mode_matches,
                    ));
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_branch_selection_matches_lean_for_all_inputs() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-branch-selection")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for existing in [false, true] {
        for present in [false, true] {
            for occupied in [false, true] {
                expected.push(fun_refactor::git::worktree_branch_selection_allowed(
                    existing, present, occupied,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn worktree_archive_compaction_matches_lean_for_all_inputs() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("worktree-archive-compaction")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for complete in [false, true] {
        for checkout_absent in [false, true] {
            for metadata_absent in [false, true] {
                for unlocked in [false, true] {
                    expected.push(fun_refactor::git::worktree_archive_compaction_allowed(
                        complete,
                        checkout_absent,
                        metadata_absent,
                        unlocked,
                    ));
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn body_replacement_budgets_match_lean_at_size_and_machine_boundaries() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("body-replacement-budget")
        .output()
        .unwrap();
    assert!(output.status.success());
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for before in [0, 1, 2, 3, 65535, 65536, 65537, usize::MAX] {
        for after in [0, 1, 2, 3, 65535, 65536, 65537, usize::MAX] {
            expected.push(fun_refactor::project::body_replacement_budget(
                before, after,
            ));
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn reviewed_plan_basis_acceptance_matches_lean_for_every_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("plan-basis")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<bool>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for complete in [false, true] {
        for supplied in [false, true] {
            for matches in [false, true] {
                expected.push(fun_refactor::project::reviewed_plan_basis_allowed(
                    complete, supplied, matches,
                ));
            }
        }
    }
    assert_eq!(actual, expected);
}

#[test]
fn handle_selection_status_matches_lean_for_every_input() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("handle-selection")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let actual = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| line.parse::<usize>().unwrap())
        .collect::<Vec<_>>();
    let mut expected = Vec::new();
    for in_scope in [false, true] {
        for declaration in [false, true] {
            for local in [false, true] {
                for include_locals in [false, true] {
                    expected.push(fun_refactor::project::handle_selection_status(
                        in_scope,
                        declaration,
                        local,
                        include_locals,
                    ));
                }
            }
        }
    }
    assert_eq!(actual, expected);
}

fn declaration_offset_samples() -> Vec<String> {
    let alphabet = ["a", " ", "\t", "\r", "\n", "é", "🙂"];
    let mut sources = vec![String::new()];
    let mut words = sources.clone();
    for _ in 0..4 {
        words = words
            .iter()
            .flat_map(|stem| alphabet.iter().map(move |c| format!("{stem}{c}")))
            .collect();
        sources.extend(words.clone());
    }
    sources.extend(
        [
            "\n\u{a0}",
            "\n\u{2003}",
            "\n\u{2028}",
            "\n\u{b}",
            "\n\u{c}",
            "mod target {\r\n  ",
            "/* } */ ",
            "\n// }",
            "\r\n\t\r ",
            "\0\n ",
        ]
        .map(str::to_owned),
    );
    sources
}

fn reverse_declaration_offset(text: &str, body_start: usize) -> usize {
    for (offset, c) in text.char_indices().rev() {
        if matches!(c, ' ' | '\t' | '\r') {
            continue;
        }
        return if c == '\n' && offset + 1 > body_start {
            offset + 1
        } else {
            text.len()
        };
    }
    text.len()
}

#[test]
fn author_selection_conflicts_match_lean_and_interval_oracle() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("selection-conflicts")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut actual = stdout.lines();
    let samples = [
        0_u64,
        1,
        2,
        3,
        4,
        79,
        80,
        499,
        500,
        65536,
        u32::MAX.into(),
        u64::MAX,
    ];
    let mut count = 0;
    for left_start in samples {
        for left_end in samples {
            if left_start > left_end {
                continue;
            }
            for right_start in samples {
                for right_end in samples {
                    if right_start > right_end {
                        continue;
                    }
                    let line = actual.next().expect("one Lean result per valid span pair");
                    let (Ok(left_start), Ok(left_end), Ok(right_start), Ok(right_end)) = (
                        usize::try_from(left_start),
                        usize::try_from(left_end),
                        usize::try_from(right_start),
                        usize::try_from(right_end),
                    ) else {
                        continue;
                    };
                    let rust = fun_refactor::project::author_selection_conflict(
                        left_start,
                        left_end,
                        right_start,
                        right_end,
                    );
                    let lean = line.parse::<bool>().unwrap();
                    let oracle = if left_start == left_end {
                        right_start <= left_start && left_start <= right_end
                    } else if right_start == right_end {
                        left_start <= right_start && right_start <= left_end
                    } else {
                        left_start.max(right_start) < left_end.min(right_end)
                    };
                    assert_eq!(
                        rust, lean,
                        "{left_start}..{left_end}, {right_start}..{right_end}"
                    );
                    assert_eq!(
                        rust, oracle,
                        "{left_start}..{left_end}, {right_start}..{right_end}"
                    );
                    count += 1;
                }
            }
        }
    }
    assert!(actual.next().is_none());
    assert_eq!(count, if usize::BITS == 64 { 6_084 } else { 4_356 });

    let unicode = "aé🙂z";
    assert!(unicode.is_char_boundary(1));
    assert!(unicode.is_char_boundary(3));
    assert!(unicode.is_char_boundary(7));
    assert!(fun_refactor::project::author_selection_conflict(1, 1, 1, 3));
    assert!(fun_refactor::project::author_selection_conflict(3, 3, 1, 3));
    assert!(!fun_refactor::project::author_selection_conflict(
        1, 3, 3, 7
    ));
    assert!(fun_refactor::project::author_selection_conflict(1, 7, 3, 7));
}

#[test]
fn declaration_insertion_offsets_match_lean_and_reverse_oracle() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("declaration-offsets")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut actual = stdout.lines();
    let mut count = 0;
    let mut compare = |text: &str, body_start: u64| {
        let line = actual.next().expect("one Lean result per placement case");
        if let Ok(body_start) = usize::try_from(body_start) {
            let rust = fun_refactor::project::declaration_insertion_offset(text, body_start);
            let lean = line.parse::<usize>().unwrap();
            assert_eq!(rust, lean, "{text:?}, {body_start}");
            assert_eq!(rust, reverse_declaration_offset(text, body_start));
            assert!(rust <= text.len() && text.is_char_boundary(rust));
            assert!(text[rust..]
                .bytes()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r')));
            if body_start < text.len() {
                assert!(body_start < rust);
            }
            let inserted = apply_to_string(
                text,
                &[Edit::new(
                    Span::new(rust, rust),
                    "fn calc() {}\n",
                    "placement",
                )],
            )
            .unwrap();
            assert_eq!(&inserted[..rust], &text[..rust]);
            assert_eq!(&inserted[rust + 13..], &text[rust..]);
            count += 1;
        }
    };
    for text in declaration_offset_samples() {
        for body_start in (0..text.len() as u64 + 2).chain([u32::MAX.into(), u64::MAX]) {
            compare(&text, body_start);
        }
    }
    for text in [
        format!("{}{{\n\t  ", "🙂".repeat(4096)),
        format!("{{\r\n{}", " ".repeat(65536)),
        format!("{{{}x", " ".repeat(4096)),
    ] {
        for body_start in [0, 1, text.len() as u64 - 1, text.len() as u64, u64::MAX] {
            compare(&text, body_start);
        }
    }
    assert!(actual.next().is_none());
    assert_eq!(count, if usize::BITS == 64 { 28_185 } else { 25_371 });
}

#[test]
fn declaration_insertion_reports_match_lean_placement() {
    build_kernel();
    for (source, selected, fragment_text, kind) in [
        ("mod target {}", "target", "fn calc() {}", "inline-module"),
        (
            "mod target {\n    }\n",
            "target",
            "fn calc() {}",
            "inline-module",
        ),
        (
            "mod outer {\r\n\tmod target {\r\n\t}\r\n}\r\n",
            "target",
            "fn calc() {}",
            "inline-module",
        ),
        (
            "// π\nmod target { /* } */ }",
            "target",
            "fn calc() {}",
            "inline-module",
        ),
        (
            "mod target {\n// }\n}\n",
            "target",
            "fn calc() {}",
            "inline-module",
        ),
        (
            "mod target {\r\n\t\r }\r\n",
            "target",
            "fn calc() {}",
            "inline-module",
        ),
        (
            "mod target { const X: &str = r#\"}\"#; }",
            "target",
            "fn calc() {}",
            "inline-module",
        ),
        (
            "mod target {\n//! Inner docs.\n}\n",
            "target",
            "fn calc() {}",
            "inline-module",
        ),
        (
            "struct Target; impl Target {\n    fn anchor(&self) {}\n}\n",
            "anchor",
            "fn calc(&self) {}",
            "impl",
        ),
        (
            "trait Contract {\r\n\tfn anchor(&self);\r\n}\r\n",
            "Contract",
            "fn calc(&self);",
            "trait",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("app.rs"), source).unwrap();
        let fragment = temp.path().join("function.txt");
        std::fs::write(&fragment, fragment_text).unwrap();
        let run = |args: &[&str]| {
            let output = Command::new(env!("CARGO_BIN_EXE_fr"))
                .args(["--json", "--no-cache", "-C"])
                .arg(&workspace)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
        };
        let found = run(&["project", "find", selected, "--in", "app.rs"]);
        let handle = found["rows"][0][0].as_str().unwrap();
        let report = run(&[
            "author",
            "insert-declaration",
            handle,
            "--from",
            fragment.to_str().unwrap(),
        ]);
        assert_eq!(report["container"]["kind"], kind);
        let body = &report["container"]["before_span"];
        let start = body["start"].as_u64().unwrap() as usize;
        let close = body["end"].as_u64().unwrap() as usize - 1;
        assert_eq!(&source[close..close + 1], "}");
        let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
            .args(["declaration-offset", &start.to_string(), &source[..close]])
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let lean = String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .parse::<usize>()
            .unwrap();
        assert_eq!(report["insertion"]["before_span"]["start"], lean);
        assert_eq!(report["insertion"]["before_span"]["end"], lean);
        assert_eq!(
            std::fs::read_to_string(workspace.join("app.rs")).unwrap(),
            source
        );
        assert!(!workspace.join(".fr-history").exists());
    }
}

#[test]
fn the_edit_kernel_accepts_a_reported_author_batch() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let source = "// π\r\nfn first() -> i32 { 1 }\r\nfn second() -> i32 { 2 }\r\n";
    std::fs::write(workspace.join("app.rs"), source).unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "--no-cache", "-C"])
            .arg(&workspace)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let map = run(&["project", "map", "--fields", "handle,name"]);
    let mut operations = Vec::new();
    let fragments = ["{ 200 }", "{ let value = 100; value }"];
    for (name, fragment) in ["second", "first"].into_iter().zip(fragments) {
        let handle = map["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[1] == name)
            .unwrap()[0]
            .as_str()
            .unwrap();
        let input = temp.path().join(format!("{name}.txt"));
        std::fs::write(&input, fragment).unwrap();
        operations.push(serde_json::json!({"op":"replace-body","handle":handle,"from":input}));
    }
    let input = temp.path().join("batch.json");
    std::fs::write(
        &input,
        serde_json::json!({"operations":operations}).to_string(),
    )
    .unwrap();
    let report = run(&["author", "batch", "--from", input.to_str().unwrap()]);
    assert_eq!(report["span_basis"], "original-source");
    let edits: Vec<_> = report["steps"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fragments)
        .map(|(step, fragment)| {
            let span = &step["before_span"];
            Edit::new(
                Span::new(
                    span["start"].as_u64().unwrap() as usize,
                    span["end"].as_u64().unwrap() as usize,
                ),
                fragment,
                "batch",
            )
        })
        .collect();
    let expected = source
        .replace("{ 1 }", fragments[1])
        .replace("{ 2 }", fragments[0]);
    assert_eq!(apply_to_string(source, &edits).unwrap(), expected);
    kernel_accepts(source, &edits, &expected);
    assert_eq!(
        std::fs::read_to_string(workspace.join("app.rs")).unwrap(),
        source
    );
}

#[test]
fn the_edit_kernel_accepts_a_reported_author_batch_with_imports() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let source = "use std::cmp::max;\n// π\nfn calc() -> i32 { 1 }\n";
    std::fs::write(workspace.join("app.rs"), source).unwrap();
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_fr"))
            .args(["--json", "--no-cache", "-C"])
            .arg(&workspace)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()
    };
    let map = run(&["project", "map", "--fields", "handle,name"]);
    let handle = |name: &str| {
        map["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row[1] == name)
            .unwrap()[0]
            .as_str()
            .unwrap()
    };
    let fragment = temp.path().join("body.txt");
    std::fs::write(&fragment, "{ 2 }").unwrap();
    let manifest = temp.path().join("batch.json");
    std::fs::write(
        &manifest,
        serde_json::json!({"operations":[
            {"op":"replace-body","handle":handle("calc"),"from":fragment},
            {"op":"organize-imports","handle":handle("app.rs")}
        ]})
        .to_string(),
    )
    .unwrap();
    let report = run(&["author", "batch", "--from", manifest.to_str().unwrap()]);
    let body = &report["steps"][0]["before_span"];
    let imports = &report["steps"][1]["imports"]["edits"][0]["before_span"];
    let edits = [
        Edit::new(
            Span::new(
                body["start"].as_u64().unwrap() as usize,
                body["end"].as_u64().unwrap() as usize,
            ),
            "{ 2 }",
            "body",
        ),
        Edit::new(
            Span::new(
                imports["start"].as_u64().unwrap() as usize,
                imports["end"].as_u64().unwrap() as usize,
            ),
            "",
            "imports",
        ),
    ];
    let expected = "// π\nfn calc() -> i32 { 2 }\n";
    assert_eq!(apply_to_string(source, &edits).unwrap(), expected);
    kernel_accepts(source, &edits, expected);
}

#[test]
fn proof_debt_ceiling_matches_lean_over_the_bounded_domain() {
    build_kernel();
    let output = Command::new(root().join("kernels/.lake/build/bin/fr-project-kernel"))
        .arg("adoption-debt")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed = String::from_utf8(output.stdout).unwrap();
    let observed = observed.lines().collect::<Vec<_>>();
    let expected = (0..65)
        .flat_map(|obligations| {
            (0..65).map(move |ceiling| {
                fun_refactor::spec::debt_within_ceiling(obligations, ceiling).to_string()
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(observed, expected);
}
