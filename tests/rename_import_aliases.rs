use fun_refactor::edit::{commit, plan as plan_edits, Validation};
use fun_refactor::index::Index;
use fun_refactor::model::ReferenceKind;
use fun_refactor::refactor::rename;
use fun_refactor::scan::ScanOptions;

#[test]
fn exported_rename_changes_the_imported_name_and_preserves_local_aliases() {
    for extension in ["ts", "tsx", "js", "jsx"] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let defining = root.join(format!("numbers.{extension}"));
        let definition = "// café\nexport function calculate(n) { return n + 3; }\n";
        std::fs::write(&defining, definition).unwrap();
        std::fs::write(
            root.join(format!("other.{extension}")),
            "export function calculate(n) { return n + 99; }\n",
        )
        .unwrap();
        let consuming = root.join(format!("app.{extension}"));
        let consumer = include_str!("fixtures/rename-import-aliases/app.ts");
        std::fs::write(&consuming, consumer).unwrap();
        let identical = root.join(format!("identical.{extension}"));
        let same_alias = "import { calculate as calculate } from './numbers';\ncalculate(4);\n";
        std::fs::write(&identical, same_alias).unwrap();
        let plain = root.join(format!("plain.{extension}"));
        let unaliased = "import { calculate } from './numbers';\ncalculate(5);\n";
        std::fs::write(&plain, unaliased).unwrap();

        let index = Index::build(root, &ScanOptions::default()).unwrap();
        let target = index.find_symbols("calculate", Some(&defining));
        assert_eq!(target.len(), 1);
        let plan = rename::plan(&index, target[0].id, "compute").unwrap();
        assert!(plan.warnings.is_empty(), "{extension}: {:?}", plan.warnings);
        commit(&plan_edits(&plan.edits, Validation::ReparseStrict).unwrap()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&defining).unwrap(),
            definition.replace("calculate", "compute")
        );
        assert_eq!(
            std::fs::read_to_string(&consuming).unwrap(),
            consumer.replacen("calculate as display", "compute as display", 1)
        );
        assert_eq!(
            std::fs::read_to_string(&identical).unwrap(),
            same_alias.replacen("calculate as calculate", "compute as calculate", 1)
        );
        assert_eq!(
            std::fs::read_to_string(&plain).unwrap(),
            unaliased.replace("calculate", "compute")
        );
        assert_eq!(
            std::fs::read_to_string(root.join(format!("other.{extension}"))).unwrap(),
            "export function calculate(n) { return n + 99; }\n"
        );
    }
}

#[test]
fn python_import_renames_follow_function_and_class_binding_scopes() {
    for aliased in [false, true] {
        for renamed_module in ["first", "second"] {
            let temporary = tempfile::tempdir().unwrap();
            let root = temporary.path();
            for (module, result) in [("first", 1), ("second", 2)] {
                std::fs::write(
                    root.join(format!("{module}.py")),
                    format!("def compute():\n    return {result}\n"),
                )
                .unwrap();
            }
            let consumer = root.join("consumer.py");
            let source = |global: &str, local: &str, local_call: &str| {
                format!(
                "from first import {global}\n\ndef outside():\n    return {global}()\n\ndef inside():\n    from second import {local}{}\n    return {local_call}()\n\nclass Container:\n    from second import {local} as compute\n\n    def check(self):\n        return {global}()\n", if aliased { " as compute" } else { "" })
            };
            std::fs::write(&consumer, source("compute", "compute", "compute")).unwrap();
            let check = || {
                let output = std::process::Command::new("python3")
                    .args(["-B", "-c", "import consumer; assert consumer.outside() == 1; assert consumer.inside() == 2; assert consumer.Container.compute() == 2; assert consumer.Container().check() == 1"])
                    .current_dir(root).output().unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            };
            check();
            let index = Index::build(root, &ScanOptions::default()).unwrap();
            let calls: Vec<_> = index
                .references
                .iter()
                .filter(|reference| {
                    reference.file == consumer
                        && reference.kind == ReferenceKind::Call
                        && reference.name == "compute"
                })
                .map(|reference| {
                    index
                        .symbol(reference.target.unwrap())
                        .unwrap()
                        .file
                        .clone()
                })
                .collect();
            assert_eq!(
                calls,
                vec![
                    root.join("first.py"),
                    root.join("second.py"),
                    root.join("first.py")
                ]
            );
            let defining = root.join(format!("{renamed_module}.py"));
            let target = index.find_symbols("compute", Some(&defining));
            assert_eq!(target.len(), 1);
            let plan = rename::plan(&index, target[0].id, "renamed").unwrap();
            assert!(plan.warnings.is_empty(), "{:?}", plan.warnings);
            commit(&plan_edits(&plan.edits, Validation::ReparseStrict).unwrap()).unwrap();
            let global = if renamed_module == "first" {
                "renamed"
            } else {
                "compute"
            };
            let local = if renamed_module == "second" {
                "renamed"
            } else {
                "compute"
            };
            let local_call = if aliased { "compute" } else { local };
            assert_eq!(
                std::fs::read_to_string(&consumer).unwrap(),
                source(global, local, local_call)
            );
            check();
        }
    }
}
