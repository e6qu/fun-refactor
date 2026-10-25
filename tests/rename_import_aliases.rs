use fun_refactor::edit::{commit, plan as plan_edits, Validation};
use fun_refactor::index::Index;
use fun_refactor::refactor::rename;
use fun_refactor::scan::ScanOptions;

#[test]
fn exported_rename_changes_the_imported_name_and_preserves_local_aliases() {
    for extension in ["ts", "tsx", "js"] {
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
