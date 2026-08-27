//! A string that names a file, and the file it names.

use fun_refactor::analysis::paths;
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;
use std::path::PathBuf;

/// A workspace on disk.
fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let root = tmp.path().to_path_buf();
    (tmp, root)
}

fn links(files: &[(&str, &str)]) -> (tempfile::TempDir, Vec<paths::PathLink>) {
    let (tmp, root) = workspace(files);
    let index = Index::build(&root, &ScanOptions::default()).unwrap();
    let found = paths::links(&index, &root).unwrap();
    (tmp, found)
}

#[test]
fn a_ci_step_finds_the_script_it_runs() {
    let (_tmp, found) = links(&[
        (
            "ci/workflow.yml",
            "jobs:\n  build:\n    steps:\n      - run: ./scripts/deploy.sh --namespace signals\n",
        ),
        ("scripts/deploy.sh", "#!/bin/sh\necho deploying\n"),
    ]);
    let step = found
        .iter()
        .find(|l| l.written.contains("deploy.sh"))
        .unwrap_or_else(|| panic!("no link to the script: {found:?}"));
    assert!(!step.is_dangling(), "{step:?}");
    assert!(
        step.names.as_ref().unwrap().ends_with("scripts/deploy.sh"),
        "{step:?}"
    );
}

#[test]
fn a_step_running_a_script_nobody_kept_says_so() {
    // The one failure a path edge can report, and the reason to report at all.
    let (_tmp, found) = links(&[(
        "ci/workflow.yml",
        "jobs:\n  build:\n    steps:\n      - run: ./scripts/gone.sh\n",
    )]);
    let step = found
        .iter()
        .find(|l| l.written.contains("gone.sh"))
        .unwrap_or_else(|| panic!("no link: {found:?}"));
    assert!(step.is_dangling(), "{step:?}");
}

#[test]
fn a_shell_command_is_not_a_path() {
    // `make`, `npm ci`, `cargo test`: a command is not a file this workspace holds.
    let (_tmp, found) = links(&[(
        "ci/workflow.yml",
        "jobs:\n  build:\n    steps:\n      - run: make\n      - run: cargo test\n",
    )]);
    assert!(found.is_empty(), "a command is not a path: {found:?}");
}

#[test]
fn terraform_finds_the_template_it_renders() {
    let (_tmp, found) = links(&[
        (
            "main.tf",
            "resource \"aws_instance\" \"a\" {\n  \
             user_data = templatefile(\"${path.module}/init.sh\", { port = 8080 })\n}\n",
        ),
        ("init.sh", "#!/bin/sh\necho starting\n"),
    ]);
    let rendered = found
        .iter()
        .find(|l| l.written.contains("init.sh"))
        .unwrap_or_else(|| panic!("no link to the template: {found:?}"));
    assert!(!rendered.is_dangling(), "{rendered:?}");
}

#[test]
fn a_url_is_not_a_path_in_this_workspace() {
    let (_tmp, found) = links(&[(
        "ci/workflow.yml",
        "jobs:\n  build:\n    steps:\n      - run: https://example.com/install.sh\n",
    )]);
    assert!(found.is_empty(), "a URL names another host: {found:?}");
}

#[test]
fn a_path_beside_the_file_is_found_before_one_at_the_root() {
    // `./init.sh` beside a `.tf` is the file in that directory.
    let (_tmp, found) = links(&[
        (
            "modules/web/main.tf",
            "resource \"a\" \"b\" {\n  user_data = templatefile(\"init.sh\", {})\n}\n",
        ),
        ("modules/web/init.sh", "#!/bin/sh\necho web\n"),
        ("init.sh", "#!/bin/sh\necho root\n"),
    ]);
    let rendered = found
        .iter()
        .find(|l| l.written.contains("init.sh"))
        .unwrap_or_else(|| panic!("no link: {found:?}"));
    assert!(
        rendered
            .names
            .as_ref()
            .unwrap()
            .ends_with("modules/web/init.sh"),
        "{rendered:?}"
    );
}

#[test]
fn a_markdown_link_finds_the_file_it_documents() {
    // Documentation drifts from code more reliably than anything else in a
    // repository, and a link to a file somebody moved is where it shows first.
    let (_tmp, found) = links(&[
        (
            "README.md",
            "See [the ingest module](src/ingest.rs) and [the gone one](src/gone.rs).\n",
        ),
        ("src/ingest.rs", "pub fn ingest() {}\n"),
    ]);
    let kept = found
        .iter()
        .find(|l| l.written.contains("ingest.rs"))
        .unwrap_or_else(|| panic!("no link: {found:?}"));
    assert!(!kept.is_dangling(), "{kept:?}");

    let moved = found
        .iter()
        .find(|l| l.written.contains("gone.rs"))
        .unwrap_or_else(|| panic!("no link: {found:?}"));
    assert!(moved.is_dangling(), "{moved:?}");
}

#[test]
fn a_markdown_link_to_another_site_names_no_file_here() {
    let (_tmp, found) = links(&[(
        "README.md",
        "See [the docs](https://example.com/guide.md) for more.\n",
    )]);
    assert!(found.is_empty(), "a URL names another host: {found:?}");
}

#[test]
fn a_markdown_link_keeps_the_file_and_drops_the_heading() {
    // `guide.md#intro` names a file and a heading inside it.
    let (_tmp, found) = links(&[
        ("README.md", "See [the guide](docs/guide.md#intro).\n"),
        ("docs/guide.md", "# Guide\n\n## Intro\n"),
    ]);
    let link = found
        .iter()
        .find(|l| l.written.contains("guide.md"))
        .unwrap_or_else(|| panic!("no link: {found:?}"));
    assert_eq!(link.written, "docs/guide.md", "{link:?}");
    assert!(!link.is_dangling(), "{link:?}");
}
