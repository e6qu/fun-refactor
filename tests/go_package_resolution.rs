//! Go resolves by package, and a package is a directory.

use fun_refactor::analysis::entrypoints::Entrypoints;
use fun_refactor::index::Index;
use fun_refactor::model::Confidence;
use fun_refactor::refactor::delete;

mod common;
use common::workspace;

fn only(index: &Index, name: &str) -> fun_refactor::model::SymbolId {
    let found = index.find_symbols(name, None);
    assert_eq!(found.len(), 1, "expected one '{name}', got {}", found.len());
    found[0].id
}

fn references_to(index: &Index, name: &str) -> Vec<Confidence> {
    index
        .references_to(only(index, name))
        .iter()
        .map(|r| r.confidence)
        .collect()
}

#[test]
fn a_function_is_visible_from_the_file_beside_it() {
    // helm: `validateNoDeprecations` in deprecations.go, called from template.go.
    let (_tmp, index) = workspace(&[
        (
            "a.go",
            "package p\n\nfunc helper(x int) int {\n\treturn x\n}\n",
        ),
        (
            "b.go",
            "package p\n\nfunc caller() int {\n\treturn helper(1)\n}\n",
        ),
    ]);
    assert_eq!(references_to(&index, "helper"), vec![Confidence::Exact]);
}

#[test]
fn a_package_level_var_is_visible_from_the_file_beside_it() {
    // helm: `aliasNameFormat` in chart.go, used in dependency.go.
    let (_tmp, index) = workspace(&[
        ("a.go", "package p\n\nvar pattern = \"x\"\n"),
        (
            "b.go",
            "package p\n\nfunc use() string {\n\treturn pattern\n}\n",
        ),
    ]);
    assert_eq!(references_to(&index, "pattern"), vec![Confidence::Exact]);
}

#[test]
fn a_package_is_a_directory_and_not_the_whole_tree() {
    let (_tmp, index) = workspace(&[
        (
            "one/a.go",
            "package one\n\nfunc helper() int {\n\treturn 1\n}\n",
        ),
        (
            "two/b.go",
            "package two\n\nfunc caller() int {\n\treturn helper()\n}\n",
        ),
    ]);
    // `helper` is unexported and in another package: nothing here can see it.
    let confidences = references_to(&index, "helper");
    assert!(
        confidences.is_empty(),
        "a different directory is a different package: {confidences:?}"
    );
}

#[test]
fn a_method_call_is_not_a_package_level_call_of_the_same_name() {
    // helm: `statuswait.go` declares both a `statusWaiter.contextWithTimeout` method and a
    // package-level `contextWithTimeout`.
    let (_tmp, index) = workspace(&[(
        "a.go",
        "package p\n\ntype waiter struct{}\n\n\
         func (w *waiter) timeout(d int) int {\n\treturn timeout(d)\n}\n\n\
         func timeout(d int) int {\n\treturn d\n}\n\n\
         func run(w *waiter) int {\n\treturn w.timeout(1)\n}\n",
    )]);

    let method = index
        .symbols
        .iter()
        .find(|s| s.name == "timeout" && s.qualifier.is_some())
        .expect("the method is indexed");
    let function = index
        .symbols
        .iter()
        .find(|s| s.name == "timeout" && s.qualifier.is_none())
        .expect("the function is indexed");

    let to_method: Vec<_> = index.references_to(method.id);
    let to_function: Vec<_> = index.references_to(function.id);
    assert_eq!(to_method.len(), 1, "w.timeout(1) is the method call");
    assert_eq!(to_function.len(), 1, "timeout(d) is the plain call");
}

#[test]
fn a_binding_is_not_in_scope_inside_its_own_initialiser() {
    // helm: `templatesDirExists := run(…, templatesDirExists(path))`.
    let (_tmp, index) = workspace(&[(
        "a.go",
        "package p\n\nfunc check(p string) bool {\n\treturn p != \"\"\n}\n\n\
         func run(p string) bool {\n\tcheck := check(p)\n\treturn check\n}\n",
    )]);
    let function = index
        .symbols
        .iter()
        .find(|s| s.name == "check" && s.kind == fun_refactor::model::SymbolKind::Function)
        .expect("the function is indexed");
    assert!(
        !index.references_to(function.id).is_empty(),
        "the call in the initialiser names the function, not the variable it declares"
    );
}

#[test]
fn a_use_binds_to_the_declaration_above_it_not_the_nearer_one_below() {
    // helm: `var ret map[string]any` … `return ret` … `ret, err := convert(m)`.
    let (_tmp, index) = workspace(&[(
        "a.go",
        "package p\n\nfunc convert(m int) (int, error) {\n\treturn m, nil\n}\n\n\
         func f(m int) int {\n\tvar ret int\n\tif m == 0 {\n\t\treturn ret\n\t}\n\n\
         \tret, err := convert(m)\n\tif err != nil {\n\t\treturn 0\n\t}\n\treturn ret\n}\n",
    )]);
    let first = index
        .symbols
        .iter()
        .filter(|s| s.name == "ret")
        .min_by_key(|s| s.name_span.start)
        .expect("both bindings are indexed");
    assert!(
        !index.references_to(first.id).is_empty(),
        "`return ret` above the re-declaration reads the first binding"
    );
}

#[test]
fn one_package_may_declare_a_name_twice_under_opposite_build_tags() {
    // helm: `renameFallback` in rename.go (!windows) and rename_windows.go (windows).
    let (_tmp, index) = workspace(&[
        (
            "rename.go",
            "//go:build !windows\n\npackage p\n\nfunc fallback() int {\n\treturn 1\n}\n",
        ),
        (
            "rename_windows.go",
            "//go:build windows\n\npackage p\n\nfunc fallback() int {\n\treturn 2\n}\n",
        ),
        (
            "use.go",
            "package p\n\nfunc caller() int {\n\treturn fallback()\n}\n",
        ),
    ]);
    let unused = delete::find_unused(&index, &Entrypoints::exactly(&[only(&index, "caller")]));
    let dead: Vec<&str> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !dead.contains(&"fallback"),
        "neither build variant is dead: {dead:?}"
    );
}

#[test]
fn everything_an_exported_symbol_reaches_is_live() {
    // helm is a library: `performInstall` <- `performInstallCtx` <- the exported
    // `RunWithContext`.
    let (_tmp, index) = workspace(&[(
        "a.go",
        "package p\n\nfunc helper() int {\n\treturn 1\n}\n\n\
         func Exported() int {\n\treturn helper()\n}\n",
    )]);
    let unused = delete::find_unused(&index, &Entrypoints::none());
    let dead: Vec<&str> = unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        !dead.contains(&"helper"),
        "code outside the workspace can call the public API: {dead:?}"
    );
    assert!(
        dead.contains(&"Exported"),
        "the report still carries the export, tagged, for the caller to judge: {dead:?}"
    );
}

#[test]
fn two_types_sharing_a_private_method_name_both_stay_live() {
    // helm: `Configuration.recordRelease` and `Install.recordRelease`.
    let (_tmp, index) = workspace(&[(
        "a.go",
        "package p\n\ntype A struct{}\ntype B struct{}\n\n\
         func (a *A) record(x int) {}\n\
         func (b *B) record(x int) {}\n\n\
         func Run(m map[string]any) {\n\tm[\"k\"].record(1)\n}\n",
    )]);
    let report = delete::find_unused_report(&index, &Entrypoints::none());
    let dead: Vec<&str> = report
        .unused
        .iter()
        .filter_map(|id| index.symbol(*id))
        .map(|s| s.name.as_str())
        .collect();
    assert!(!dead.contains(&"record"), "both stay live: {dead:?}");

    let spared = index
        .symbols
        .iter()
        .find(|s| s.name == "record")
        .and_then(|s| report.explain(&index, s.id))
        .expect("and it says why");
    assert!(spared.contains("more than one definition"), "got: {spared}");
}
