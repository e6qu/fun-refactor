use fun_refactor::edit::{apply_to_string, Edit};
use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::{scan, ScanOptions};
use fun_refactor::span::Span;
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
    let mut program =
        String::from("import FrKernels.Edit\n\nset_option maxRecDepth 5000\n\nopen FrKernels\n");
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

    let checks: Vec<(String, Vec<Edit>, String)> = plan
        .edits
        .iter()
        .flat_map(|(path, edits)| {
            let source = std::fs::read_to_string(path).expect("read fr source");
            kernel_windows(&source, edits)
        })
        .collect();
    assert!(
        checks.len() > 1,
        "the self rename changes multiple fr files"
    );
    assert_eq!(
        checks
            .iter()
            .map(|(_, edits, _)| edits.len())
            .sum::<usize>(),
        plan.edits.edit_count(),
        "every self-rename edit reaches the Lean audit"
    );
    let lean_checks: Vec<(&str, &[Edit], &str)> = checks
        .iter()
        .map(|(source, edits, expected)| (source.as_str(), edits.as_slice(), expected.as_str()))
        .collect();
    kernel_accepts_all(&lean_checks);
}
