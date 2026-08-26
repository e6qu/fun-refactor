//! The open entries in BUGS.md, held to what they say.
//!
//! Eight of the thirteen are limits of the published grammars, and
//! `tests/known_grammar_gaps.rs` pins every one of those from both sides. The rest are
//! this tool's own behaviour, and they used to be prose. A description of what happens,
//! with nothing to notice when it stopped happening. B11 said `@content` was a gap after it had
//! stopped being one, and nothing noticed for months.
//!
//! Each test here asserts the *whole* of its entry: what the tool does not do, and what
//! it does instead. Every one of these stands on the second half. An incomplete answer
//! that says so is a different thing from a wrong one. A test that checked only the
//! incompleteness would pass just as well if the report went away.
//!
//! A failure here means the entry is out of date. The entry is what to update.

use fun_refactor::index::Index;
use fun_refactor::model::SymbolId;
use fun_refactor::scan::{scan, ScanOptions};
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    let root = dir.path().to_path_buf();
    (dir, root)
}

fn index_of(root: &Path) -> Index {
    let scanned = scan(root, &ScanOptions::default()).expect("scan");
    Index::build_from_scan(&scanned).expect("index")
}

fn symbol(index: &Index, name: &str) -> SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

#[test]
fn dispatch_is_followed_as_far_as_the_source_shows_it() {
    // B5. A call through a trait object resolves to no implementation, so reachability fans it
    // out to every type that declares itself an implementation. A call through a field reaches
    // whatever function the source puts behind that field. What is left is a function nothing
    // names: assembled at runtime, or supplied by a caller this workspace does not hold.
    let (_tmp, root) = workspace(&[
        (
            "a.rs",
            "pub trait Shape {\n    fn area(&self) -> f64;\n}\npub struct Circle;\n\
             impl Shape for Circle {\n    fn area(&self) -> f64 {\n        1.0\n    }\n}\n\
             pub fn total(s: &dyn Shape) -> f64 {\n    s.area()\n}\n",
        ),
        (
            "b.rs",
            "pub struct Held {\n    pub run: fn() -> f64,\n}\n\
             pub fn go(h: &Held) -> f64 {\n    (h.run)()\n}\n\
             pub fn candidate() -> f64 {\n    2.0\n}\n\
             pub fn make() -> Held {\n    Held { run: candidate }\n}\n",
        ),
        (
            "c.rs",
            "pub struct Table {\n    pub perform: fn() -> f64,\n}\n\
             pub fn dispatch(t: &Table) -> f64 {\n    (t.perform)()\n}\n\
             pub fn unnamed() -> f64 {\n    3.0\n}\n",
        ),
    ]);
    let index = index_of(&root);
    let entrypoints =
        fun_refactor::analysis::entrypoints::Entrypoints::detect(&index).expect("entry points");
    let dead: Vec<&str> = fun_refactor::refactor::delete::find_unused(&index, &entrypoints)
        .into_iter()
        .filter_map(|id| index.symbol(id))
        .map(|s| s.name.as_str())
        .collect();

    assert!(
        !dead.contains(&"area"),
        "the implementation is reached through the trait, so it is not dead: {dead:?}"
    );
    assert!(
        !dead.contains(&"candidate"),
        "the source puts `candidate` behind `run`, and `go` calls `run`: {dead:?}"
    );
    assert!(
        dead.contains(&"unnamed"),
        "B5's remaining half is a function the source never names, and it is still listed. \
         If it no longer is, update the entry: {dead:?}"
    );

    // And the edge itself, which is what `fr callees` prints. It is a candidate and not a
    // resolved call, because which function sits behind the field is settled at run time.
    let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
    let go = symbol(&index, "go");
    let candidate = symbol(&index, "candidate");
    let reached: Vec<SymbolId> = graph.callees(go).into_iter().map(|(id, _)| id).collect();
    assert!(
        reached.contains(&candidate),
        "`go` reaches `candidate` through the field: {reached:?}"
    );
    assert_eq!(
        graph.origin_breakdown().get("function-value").copied(),
        Some(1),
        "and the edge says where it came from"
    );
}

#[test]
fn a_values_answer_names_the_channel_it_was_never_told_about() {
    // B13. Given some of the inputs and not others, the competition is decided *given the
    // inputs supplied*. The answer names the channel that was left out. Given none, nothing is
    // decided at all. Neither one infers an invocation.
    let (_tmp, root) = workspace(&[
        ("Chart.yaml", "name: chart\nversion: 0.1.0\n"),
        ("values.yaml", "replicas: 1\n"),
        (
            "templates/deploy.yaml",
            "apiVersion: apps/v1\nkind: Deployment\nspec:\n  replicas: {{ .Values.replicas }}\n",
        ),
    ]);
    let index = index_of(&root);
    let key = symbol(&index, "replicas");
    use fun_refactor::analysis::provenance::{self, SetFlags, ValuesInputs};

    // The report says this in its stops: a channel outside the workspace that could
    // pre-empt every source listed is a stop, and so is a competition the supplied inputs
    // settle. Both name what they were never told about.
    let said = |report: &provenance::Provenance| -> String {
        report
            .stops
            .iter()
            .map(|(_, reason)| reason.to_string())
            .collect::<Vec<_>>()
            .join(" | ")
    };

    let nothing_supplied = ValuesInputs::parse(&[], SetFlags::default()).expect("no inputs");
    let without =
        provenance::provenance_with_inputs(&index, key, 5, &nothing_supplied).expect("a report");
    assert!(
        said(&without).contains("overridden externally"),
        "with no inputs the answer is undecided and says so: {}",
        said(&without)
    );

    let some_supplied =
        ValuesInputs::parse(&[], SetFlags::of(&["replicas=3".to_string()])).expect("one --set");
    let with =
        provenance::provenance_with_inputs(&index, key, 5, &some_supplied).expect("a report");
    assert!(
        said(&with).contains("given the inputs supplied"),
        "with some inputs the answer is decided given them, and names what is missing: {}",
        said(&with)
    );
}

#[test]
fn a_call_through_a_name_reaches_every_function_put_behind_that_name() {
    // The edge is keyed by the name, and two types may hold a field of the same name.
    // A receiver whose type is settled reaches only its own record's binding.
    // `call` takes `a: &A`, so `(a.run)()` reaches `one` and not the `run` a `B`
    // literal named. Through a receiver nothing types, the name-keyed fan-out remains, and
    // the edge carries the label that says so.
    let (_tmp, root) = workspace(&[(
        "a.rs",
        "pub struct A {\n    pub run: fn() -> f64,\n}\npub struct B {\n    pub run: fn() -> f64,\n}\n         pub fn one() -> f64 {\n    1.0\n}\npub fn two() -> f64 {\n    2.0\n}\n         pub fn build() -> (A, B) {\n    (A { run: one }, B { run: two })\n}\n         pub fn call(a: &A) -> f64 {\n    (a.run)()\n}\n         pub fn blind<T>(h: &T, pick: fn(&T) -> fn() -> f64) -> f64 {\n    (pick(h))()\n}\n",
    )]);
    let index = index_of(&root);
    let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
    let reached: Vec<SymbolId> = graph
        .callees(symbol(&index, "call"))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(
        reached.contains(&symbol(&index, "one")),
        "`A`'s own binding is reached: {reached:?}"
    );
    assert!(
        !reached.contains(&symbol(&index, "two")),
        "the `run` a `B` literal named is not `a`'s: {reached:?}"
    );
    for (_, edge) in graph.callees(symbol(&index, "call")) {
        assert_eq!(edge.origin.as_str(), "function-value");
        assert!(edge.origin.is_dispatch(), "not a resolved call");
    }
}

#[test]
fn an_untyped_receiver_still_reaches_every_function_behind_the_name() {
    // The same two records, called through a field read the tool cannot type. The
    // name-keyed fan-out stays, unsound by design in the same way class-hierarchy
    // fan-out is, and labelled as a candidate.
    let (_tmp, root) = workspace(&[(
        "a.rs",
        "pub struct A {\n    pub run: fn() -> f64,\n}\npub struct B {\n    pub run: fn() -> f64,\n}\n         pub fn one() -> f64 {\n    1.0\n}\npub fn two() -> f64 {\n    2.0\n}\n         pub fn build() -> (A, B) {\n    (A { run: one }, B { run: two })\n}\n         pub fn call(pair: &(A, B)) -> f64 {\n    (pair.0.run)()\n}\n",
    )]);
    let index = index_of(&root);
    let graph = fun_refactor::analysis::call_graph::CallGraph::build(&index);
    let reached: Vec<SymbolId> = graph
        .callees(symbol(&index, "call"))
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(reached.contains(&symbol(&index, "one")), "{reached:?}");
    assert!(
        reached.contains(&symbol(&index, "two")),
        "nothing types `pair.0`, so the name reaches both: {reached:?}"
    );
}

#[test]
fn a_maps_key_and_value_types_do_not_cross() {
    // B755. The vocabularies read and the writers spell them, so a map written
    // the way its own language writes one crosses and runs. What does not cross
    // is the type it holds, and every remaining failure is that one thing.
    use fun_refactor::lang::Language;
    use fun_refactor::transpile;

    let (_tmp, root) = workspace(&[(
        "m.rs",
        "use std::collections::HashMap;\n\nfn main() {\n    \
         let mut ages: HashMap<String, i64> = HashMap::new();\n    \
         ages.insert(\"ada\".to_string(), 36);\n    \
         println!(\"{}\", ages.len());\n}\n",
    )]);
    let out = transpile::plan(&root.join("m.rs"), Language::Python).expect("a draft");
    assert!(
        out.output.contains("ages: dict[str, int] = {}")
            && out.output.contains("ages[str(\"ada\")] = 36")
            && out.output.contains("len(ages)"),
        "the constructor, the insert and the count all cross:\n{}",
        out.output
    );

    // The value type is what is left. Go is given `any`, so arithmetic on a
    // read does not compile. A Go map literal with its type written on it is
    // not read as a map at all.
    let (_tmp2, root2) = workspace(&[(
        "m.py",
        "def main() -> None:\n    ages = {\"ada\": 36}\n    total = 0\n    \
         for name in [\"ada\"]:\n        total = total + ages[name]\n    \
         print(total)\n\n\nmain()\n",
    )]);
    let go = transpile::plan(&root2.join("m.py"), Language::Go).expect("a draft");
    assert!(
        go.output.contains("map[string]any"),
        "the value type is `any`, which is why the arithmetic fails:\n{}",
        go.output
    );

    let (_tmp3, root3) = workspace(&[(
        "m.go",
        "package main\n\nfunc main() {\n\tages := map[string]int64{\"ada\": 36}\n\t\
         _ = ages\n}\n",
    )]);
    let back = transpile::plan(&root3.join("m.go"), Language::Python).expect("a draft");
    assert!(
        back.output.contains(transpile::MARKER),
        "a typed composite literal is carried, not read as a map:\n{}",
        back.output
    );
}
