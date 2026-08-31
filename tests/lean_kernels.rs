use fun_refactor::edit::{apply_to_string, Edit};
use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};
use fun_refactor::span::Span;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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

fn kernel_accepts(source: &str, edits: &[Edit], expected: &str) {
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
    let program = format!(
        "-- fn generated plan\nimport FrKernels.Edit\n\nopen FrKernels\n\ndef source : String := {}\ndef edits : List Edit := [{}]\n\nexample : applyChecked source edits = some {} := by decide\n",
        lean_string(source),
        edits,
        lean_string(expected),
    );
    let dir = tempfile::tempdir().expect("temporary Lean plan");
    let file = dir.path().join("plan.lean");
    std::fs::write(&file, program).expect("write Lean plan");
    let output = Command::new("lake")
        .args(["env", "lean", file.to_str().expect("UTF-8 temporary path")])
        .current_dir(root().join("kernels"))
        .output()
        .expect("Lean is installed for the kernel gate");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_lossless_edit_kernel_agrees_with_rust() {
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
