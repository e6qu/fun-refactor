use fun_refactor::edit::{apply_to_string, Edit};
use fun_refactor::span::Span;
use std::path::PathBuf;
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn edits_for(source: &str) -> Vec<(usize, usize, &'static str)> {
    let mut edits = Vec::new();
    for start in 0..=source.len() {
        for width in 0..=source.len() - start {
            for replacement in ["", "X", "YZ"] {
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
    let expected: Vec<String> = ["", "a", "ab", "abc", "abcd"]
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
