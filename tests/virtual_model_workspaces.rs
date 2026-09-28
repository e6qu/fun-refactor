use fun_refactor::{spec, vfs};
use serde_json::{json, Value};
use std::path::Path;

const RUST: &str = "pub fn allow(a: bool, b: bool) -> bool { a && b }\n";
const PYTHON: &str = "def allow(a: bool, b: bool) -> bool:\n    return a and b\n";

#[test]
fn bounded_reads_match_backings_and_count_bytes() {
    let disk = tempfile::tempdir().unwrap();
    let path = disk.path().join("source");
    let text = "aλ";
    std::fs::write(&path, text).unwrap();
    let memory = vfs::new_handle([(path.clone(), text.into())]);
    let check = || {
        assert_eq!(vfs::read_bounded(&path, 3).unwrap(), text.as_bytes());
        assert_eq!(
            vfs::read_to_string_bounded(&path, usize::MAX).unwrap(),
            text
        );
        for limit in [0, 1, 2] {
            let error = vfs::read_bounded(&path, limit).unwrap_err();
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
            assert!(error
                .to_string()
                .contains(&format!("exceeds {limit} bytes")));
        }
        assert_eq!(
            vfs::read_bounded(disk.path().join("missing"), 0)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    };
    check();
    vfs::with_handle(&memory, check);
    std::fs::write(&path, []).unwrap();
    assert_eq!(vfs::read_bounded(&path, 0).unwrap(), []);
    vfs::with_handle(&vfs::new_handle([(path.clone(), String::new())]), || {
        assert_eq!(vfs::read_to_string_bounded(&path, 0).unwrap(), "");
    });
    std::fs::write(&path, [255]).unwrap();
    assert_eq!(vfs::read_bounded(&path, 1).unwrap(), [255]);
    assert_eq!(
        vfs::read_to_string_bounded(&path, 1).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[test]
fn scopes_restore_the_immediate_workspace_even_after_error_or_panic() {
    let disk = tempfile::tempdir().unwrap();
    let path = disk.path().join("file");
    std::fs::write(&path, "disk").unwrap();
    let outer = vfs::new_handle([(path.clone(), "outer".into())]);
    let inner = vfs::new_handle([(path.clone(), "inner".into())]);
    assert!(!vfs::is_in_memory());
    vfs::with_handle(&outer, || {
        let result: Result<(), &str> = vfs::with_handle(&inner, || {
            assert_eq!(vfs::read_to_string(&path).unwrap(), "inner");
            vfs::write(&path, "changed").unwrap();
            Err("refused")
        });
        assert_eq!(result, Err("refused"));
        assert_eq!(vfs::read_to_string(&path).unwrap(), "outer");
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            vfs::with_handle(&inner, || {
                assert_eq!(vfs::read_to_string(&path).unwrap(), "changed");
                vfs::use_filesystem();
                panic!("test unwind");
            });
        }));
        assert!(panic.is_err());
        assert!(vfs::is_in_memory());
        assert_eq!(vfs::read_to_string(&path).unwrap(), "outer");
    });
    assert!(!vfs::is_in_memory());
    assert_eq!(vfs::read_to_string(&path).unwrap(), "disk");
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        vfs::with_handle(&outer, || panic!("test native unwind"));
    }));
    assert!(panic.is_err());
    assert!(!vfs::is_in_memory());
}

#[test]
fn snapshots_match_disk_and_memory_without_a_real_workspace_directory() {
    let disk = tempfile::tempdir().unwrap();
    let virtual_root = Path::new("__fr_nonexistent_virtual_workspace__");
    for (name, text) in [("subject.rs", RUST), ("subject.py", PYTHON)] {
        std::fs::write(disk.path().join(name), text).unwrap();
        let target = format!("{name}::allow");
        let native = spec::refinement::capture(disk.path(), &target).unwrap();
        let workspace = vfs::new_handle([(virtual_root.join(name), text.into())]);
        let virtual_snapshot = vfs::with_handle(&workspace, || {
            spec::refinement::capture(virtual_root, &target).unwrap()
        });
        assert_eq!(
            serde_json::to_value(native).unwrap(),
            serde_json::to_value(virtual_snapshot).unwrap()
        );
        assert!(!vfs::is_in_memory());
    }
}

#[test]
fn virtual_comparison_preview_preserves_source_and_checks_package_conflicts() {
    let root = Path::new("__fr_virtual_comparison__");
    let workspace = vfs::new_handle([(root.join("subject.rs"), RUST.into())]);
    vfs::with_handle(&workspace, || {
        let before = spec::refinement::capture(root, "subject.rs::allow").unwrap();
        let initial = spec::init(root, Path::new("specs")).unwrap();
        for file in initial.files {
            assert!(!file.existing);
            vfs::write(file.path, file.content).unwrap();
        }
        let request = root.join("comparison.json");
        vfs::write(
            &request,
            serde_json::to_string(&json!({
                "schema":"fr-model-comparison-1", "name":"Virtual", "before":before,
                "after":"subject.rs::allow", "relation":"equivalent", "arguments":[0,1]
            }))
            .unwrap(),
        )
        .unwrap();
        let plan = spec::refinement::prepare(root, &request, Path::new("specs")).unwrap();
        assert_eq!(plan.files.len(), 2);
        assert_eq!(plan.report["source_implementation_proved"], false);
        assert!(plan.files.iter().all(|file| !vfs::exists(&file.path)));
        assert_eq!(vfs::read_to_string(root.join("subject.rs")).unwrap(), RUST);
        let retained: Value = serde_json::from_str(&plan.files[1].updated).unwrap();
        assert_eq!(retained["request"]["before"], retained["after"]);
        vfs::write(root.join("specs/lean-toolchain"), "conflict").unwrap();
        assert!(
            spec::refinement::prepare(root, &request, Path::new("specs"))
                .err()
                .unwrap()
                .to_string()
                .contains("refusing to replace")
        );
    });
    assert!(!vfs::is_in_memory());
}

#[test]
fn source_limit_is_checked_before_parsing_on_disk_and_in_memory() {
    let disk = tempfile::tempdir().unwrap();
    for size in [65536, 65537] {
        let text = format!("{RUST}{}", " ".repeat(size - RUST.len()));
        let path = disk.path().join("subject.rs");
        std::fs::write(&path, &text).unwrap();
        let check = || {
            let result = spec::refinement::capture(disk.path(), "subject.rs::allow");
            if size == 65536 {
                assert!(result.is_ok());
            } else {
                assert!(result
                    .err()
                    .unwrap()
                    .to_string()
                    .contains("exceeds 65536 bytes"));
            }
        };
        check();
        let workspace = vfs::new_handle([(path, text)]);
        vfs::with_handle(&workspace, check);
    }
}

#[cfg(unix)]
#[test]
fn native_snapshot_paths_still_refuse_symlink_traversal() {
    let disk = tempfile::tempdir().unwrap();
    std::fs::write(disk.path().join("source.rs"), RUST).unwrap();
    std::os::unix::fs::symlink("source.rs", disk.path().join("link.rs")).unwrap();
    let error = spec::refinement::capture(disk.path(), "link.rs::allow")
        .err()
        .unwrap();
    assert!(error.to_string().contains("symlinks"));
}
