//! A `--flag` a script passes, and the program that declares it.

use fun_refactor::analysis::flags;
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

fn found(files: &[(&str, &str)]) -> (tempfile::TempDir, Vec<flags::FlagUse>) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    let index = Index::build(&root, &ScanOptions::default()).unwrap();
    let flags = flags::flags(&index).unwrap();
    (tmp, flags)
}

fn one<'a>(all: &'a [flags::FlagUse], flag: &str) -> &'a flags::FlagUse {
    all.iter().find(|f| f.flag == flag).unwrap_or_else(|| {
        panic!(
            "no flag {flag:?}: {:?}",
            all.iter().map(|f| &f.flag).collect::<Vec<_>>()
        )
    })
}

#[test]
fn a_script_flag_finds_the_clap_attribute_that_names_it() {
    let (_tmp, all) = found(&[
        (
            "src/main.rs",
            "pub struct Args {\n    #[arg(long = \"retention-days\")]\n    \
             pub days: u32,\n}\n",
        ),
        ("run.sh", "#!/bin/sh\n./collector --retention-days 30\n"),
    ]);
    let flag = one(&all, "retention-days");
    assert_eq!(flag.declared.len(), 1, "{flag:?}");
    assert_eq!(flag.passed.len(), 1, "{flag:?}");
    assert!(!flag.is_undeclared(), "{flag:?}");
}

#[test]
fn a_bare_clap_long_takes_the_field_name() {
    // `#[arg(long)]` names no flag; clap kebab-cases the field under it.
    let (_tmp, all) = found(&[
        (
            "src/main.rs",
            "pub struct Args {\n    #[arg(long)]\n    pub retention_days: u32,\n}\n",
        ),
        ("run.sh", "#!/bin/sh\n./collector --retention-days 30\n"),
    ]);
    let flag = one(&all, "retention-days");
    assert_eq!(flag.declared.len(), 1, "{flag:?}");
    assert!(!flag.is_undeclared(), "{flag:?}");
}

#[test]
fn a_go_flag_declaration_is_found() {
    let (_tmp, all) = found(&[
        (
            "main.go",
            "package main\n\nimport \"flag\"\n\nfunc main() {\n\t\
             days := flag.Int(\"retention-days\", 30, \"how long\")\n\t_ = days\n}\n",
        ),
        ("run.sh", "#!/bin/sh\n./collector --retention-days 30\n"),
    ]);
    let flag = one(&all, "retention-days");
    assert_eq!(flag.declared.len(), 1, "{flag:?}");
}

#[test]
fn an_argparse_declaration_is_found() {
    let (_tmp, all) = found(&[
        (
            "cli.py",
            "import argparse\n\nparser = argparse.ArgumentParser()\n\
             parser.add_argument(\"--retention-days\", type=int)\n",
        ),
        ("run.sh", "#!/bin/sh\n./collector --retention-days 30\n"),
    ]);
    let flag = one(&all, "retention-days");
    assert_eq!(flag.declared.len(), 1, "{flag:?}");
}

#[test]
fn a_flag_a_script_passes_and_nothing_declares_says_so() {
    // The failure worth reporting.
    let (_tmp, all) = found(&[
        (
            "src/main.rs",
            "pub struct Args {\n    #[arg(long = \"retention\")]\n    pub days: u32,\n}\n",
        ),
        ("run.sh", "#!/bin/sh\n./collector --retention-days 30\n"),
    ]);
    let stale = one(&all, "retention-days");
    assert!(stale.is_undeclared(), "{stale:?}");
    let renamed = one(&all, "retention");
    assert!(renamed.is_unpassed(), "{renamed:?}");
}

#[test]
fn a_ci_step_passing_a_flag_counts_as_passing_it() {
    let (_tmp, all) = found(&[
        (
            "src/main.rs",
            "pub struct Args {\n    #[arg(long = \"namespace\")]\n    pub ns: String,\n}\n",
        ),
        (
            "ci/build.yml",
            "jobs:\n  a:\n    steps:\n      - run: ./collector --namespace signals\n",
        ),
    ]);
    let flag = one(&all, "namespace");
    assert_eq!(flag.passed.len(), 1, "{flag:?}");
    assert!(!flag.is_undeclared(), "{flag:?}");
}

#[test]
fn a_flag_written_with_an_equals_names_the_flag_before_it() {
    let (_tmp, all) = found(&[("run.sh", "#!/bin/sh\n./collector --retention-days=30\n")]);
    let flag = one(&all, "retention-days");
    assert_eq!(flag.passed.len(), 1, "{flag:?}");
}

#[test]
fn a_bare_double_dash_names_no_flag() {
    // `--` ends the options; it is not one.
    let (_tmp, all) = found(&[("run.sh", "#!/bin/sh\n./collector -- file.txt\n")]);
    assert!(
        all.iter().all(|f| !f.flag.is_empty()),
        "{:?}",
        all.iter().map(|f| &f.flag).collect::<Vec<_>>()
    );
}

#[test]
fn a_double_dash_in_code_is_not_a_flag() {
    // A Rust comment marker and a decrement are not command lines, so only the
    // languages that write one carry a use.
    let (_tmp, all) = found(&[(
        "src/lib.rs",
        "pub fn f() {\n    // -- a note\n    let mut n = 2;\n    n -= 1;\n}\n",
    )]);
    assert!(
        all.is_empty(),
        "{:?}",
        all.iter().map(|f| &f.flag).collect::<Vec<_>>()
    );
}

#[test]
fn a_yaml_document_separator_names_no_flag() {
    // Stripping two dashes off `---` left a flag called `-`.
    let (_tmp, all) = found(&[(
        "chart/templates/service.yaml",
        "kind: Service\n---\napiVersion: networking.k8s.io/v1\nkind: Ingress\n",
    )]);
    assert!(
        all.is_empty(),
        "{:?}",
        all.iter().map(|f| &f.flag).collect::<Vec<_>>()
    );
}

#[test]
fn a_declaration_inside_a_string_literal_does_not_name_an_escaped_flag() {
    // The reader kept the backslash of an escaped quote, declaring `retention-days\`.
    let (_tmp, all) = found(&[(
        "src/main.rs",
        "const FIXTURE: &str = \"#[arg(long = \\\"retention-days\\\")]\";\n",
    )]);
    assert!(
        all.iter().all(|f| !f.flag.contains('\\')),
        "a flag name carries a backslash: {:?}",
        all.iter().map(|f| &f.flag).collect::<Vec<_>>()
    );
}

#[test]
fn a_flag_name_opens_with_a_letter_or_a_digit() {
    // One rule, and every reader holds to it.
    let (_tmp, all) = found(&[(
        "run.sh",
        "#!/bin/sh\n./collector --- --=x --2fa --retention-days 30\n",
    )]);
    for flag in all.iter().map(|f| &f.flag) {
        assert!(
            flag.starts_with(|c: char| c.is_ascii_alphanumeric()),
            "{flag:?} does not open with a letter or a digit"
        );
    }
    assert!(
        all.iter().any(|f| f.flag == "retention-days") && all.iter().any(|f| f.flag == "2fa"),
        "the real flags went missing: {:?}",
        all.iter().map(|f| &f.flag).collect::<Vec<_>>()
    );
}
