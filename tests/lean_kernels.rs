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
    use fun_refactor::history::{matches_snapshot, Snapshot};
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
        }),
        Some(Snapshot {
            content: "λ\n".to_string(),
            mode: 0o600,
        }),
        Some(Snapshot {
            content: "λ\n".to_string(),
            mode: 0o751,
        }),
        Some(Snapshot {
            content: "名".to_string(),
            mode: 0o644,
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
    use fun_refactor::history::{matches_patch_basis, matches_snapshot, Snapshot};
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
            }));
        }
    }
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
    assert_eq!(expected.len(), 1_681);
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
