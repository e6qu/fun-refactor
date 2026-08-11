//! Which Go types implement which Go interfaces.
//!
//! Go decides this by signature. The hierarchy pass compared method *name and arity*,
//! under a comment claiming a covered method set "is the whole of what implementing an
//! interface means there", which is true of Go and was not true of the code. In
//! helm/helm that produced 7,179 dispatch edges between types that do not implement each
//! other, 35% of the layer.

use fun_refactor::analysis::call_graph::CallGraph;
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

fn reaches(source: &str, from: &str) -> Vec<String> {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(tmp.path().join("a.go"), source).expect("the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.qualified_name() == from || s.name == from)
        .unwrap_or_else(|| panic!("no `{from}`"))
        .id;
    CallGraph::build(&index)
        .callees(id)
        .into_iter()
        .filter_map(|(to, _)| index.symbol(to).map(|s| s.qualified_name()))
        .collect()
}

#[test]
fn a_different_return_type_is_not_an_implementation() {
    // `Report.Run() string` does not satisfy `Run() error`. Go says so; the tool said
    // they were both candidates because both are called `Run` and take nothing.
    let source = "\
package main

type Runner interface {
\tRun() error
}

type Server struct{}

func (s Server) Run() error { return nil }

type Report struct{}

func (r Report) Run() string { return \"\" }

func start(r Runner) error {
\treturn r.Run()
}
";
    let found = reaches(source, "start");
    assert!(
        found.iter().any(|f| f == "Server::Run"),
        "the real implementation is missing: {found:?}"
    );
    assert!(
        !found.iter().any(|f| f == "Report::Run"),
        "a method with the wrong return type is still linked: {found:?}"
    );
}

#[test]
fn a_different_parameter_type_is_not_an_implementation() {
    let source = "\
package main

type Store interface {
\tPut(key string) error
}

type Disk struct{}

func (d Disk) Put(key string) error { return nil }

type Counter struct{}

func (c Counter) Put(key int) error { return nil }

func save(s Store) error {
\treturn s.Put(\"k\")
}
";
    let found = reaches(source, "save");
    assert!(found.iter().any(|f| f == "Disk::Put"), "{found:?}");
    assert!(!found.iter().any(|f| f == "Counter::Put"), "{found:?}");
}

#[test]
fn a_parameter_name_is_not_part_of_the_question() {
    // Go compares types, not names. Refusing an implementation because it called its
    // parameter something else would drop a true edge, and a dropped edge here becomes
    // a live method reported as dead code.
    let source = "\
package main

type Store interface {
\tPut(key string) error
}

type Disk struct{}

func (d Disk) Put(name string) error { return nil }

func save(s Store) error {
\treturn s.Put(\"k\")
}
";
    let found = reaches(source, "save");
    assert!(
        found.iter().any(|f| f == "Disk::Put"),
        "a differently-named parameter lost the edge: {found:?}"
    );
}

#[test]
fn an_interface_with_several_methods_needs_all_of_them() {
    let source = "\
package main

type Both interface {
\tOpen() error
\tClose() error
}

type Full struct{}

func (f Full) Open() error  { return nil }
func (f Full) Close() error { return nil }

type Half struct{}

func (h Half) Open() error { return nil }

func use(b Both) error {
\treturn b.Open()
}
";
    let found = reaches(source, "use");
    assert!(found.iter().any(|f| f == "Full::Open"), "{found:?}");
    assert!(!found.iter().any(|f| f == "Half::Open"), "{found:?}");
}

#[test]
fn the_package_a_type_is_written_from_is_not_part_of_the_question() {
    // `kube.ResourceList` from outside the package and `ResourceList` from inside are
    // the same type. Comparing the signatures as written refused
    // `PrintingKubeClient` as an implementation of an interface it plainly satisfies,
    // seven of them in helm, each one a live method that would have been reported dead.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::create_dir_all(tmp.path().join("kube")).expect("the directory");
    std::fs::write(
        tmp.path().join("kube/interface.go"),
        "package kube\n\ntype List struct{}\n\ntype Client interface {\n\tDelete(r List) error\n}\n",
    )
    .expect("the interface");
    std::fs::write(
        tmp.path().join("fake.go"),
        "package fake\n\nimport \"example/kube\"\n\ntype Printer struct{}\n\n\
         func (p Printer) Delete(r kube.List) error { return nil }\n\n\
         func run(c kube.Client) error {\n\treturn c.Delete(kube.List{})\n}\n",
    )
    .expect("the implementation");

    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.qualified_name() == "run")
        .expect("no `run`")
        .id;
    let found: Vec<String> = CallGraph::build(&index)
        .callees(id)
        .into_iter()
        .filter_map(|(to, _)| index.symbol(to).map(|s| s.qualified_name()))
        .collect();
    assert!(
        found.iter().any(|f| f == "Printer::Delete"),
        "the qualifier lost a real implementation: {found:?}"
    );
}
