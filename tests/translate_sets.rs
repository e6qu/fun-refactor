//! A set is a set in every target, whatever each one calls it.
//!
//! Four of these six languages name a set. Go and Zig spell one as a map whose
//! values carry nothing, `map[T]struct{}` and `HashMap(K, void)`, because
//! membership is all such a map can answer. Without a set in the IR, every one
//! of these crossed as something else. `set()` became a call to a function
//! named `set`. `seen.add(x)` became a store of `None`, and Go's
//! `_, ok := m[k]` became a pair nobody had.

use fun_refactor::lang::Language;
use fun_refactor::transpile;

fn translated(source: &str, name: &str, target: Language) -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, source).unwrap();
    let out = tmp.path().join("out.txt");
    transpile::plan_to(&path, target, Some(&out), false)
        .expect("a plan")
        .output
}

const SEEN_PY: &str = "\
def main() -> None:
    seen = set()
    seen.add(\"ada\")
    print(len(seen))
";

#[test]
fn a_set_is_built_the_way_each_target_builds_one() {
    for (target, expected) in [
        (Language::Rust, "std::collections::HashSet::new()"),
        (Language::TypeScript, "new Set()"),
        (Language::Java, "new HashSet<>()"),
        (Language::Go, "map[string]struct{}{}"),
    ] {
        let out = translated(SEEN_PY, "seen.py", target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
    // Zig's sets go through an allocator, so the binding is where one is built.
    let zig = translated(SEEN_PY, "seen.py", Language::Zig);
    assert!(
        zig.contains("std.StringHashMap(void).init(std.heap.page_allocator)"),
        "zig:\n{zig}"
    );
}

#[test]
fn adding_a_member_is_spelled_the_way_each_target_spells_it() {
    for (target, expected) in [
        (Language::Rust, "seen.insert(\"ada\".to_string())"),
        (Language::TypeScript, "seen.add(\"ada\")"),
        (Language::Java, "seen.add(\"ada\")"),
        (Language::Go, "seen[\"ada\"] = struct{}{}"),
        (Language::Zig, "seen.put(\"ada\", {}) catch unreachable"),
    ] {
        let out = translated(SEEN_PY, "seen.py", target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}

#[test]
fn a_set_says_how_many_members_it_has() {
    for (target, expected) in [
        (Language::Rust, "seen.len()"),
        (Language::TypeScript, "seen.size"),
        (Language::Java, "seen.size()"),
        (Language::Go, "len(seen)"),
        (Language::Zig, "seen.count()"),
    ] {
        let out = translated(SEEN_PY, "seen.py", target);
        assert!(out.contains(expected), "{target}:\n{out}");
    }
}

#[test]
fn go_asks_about_membership_in_the_only_place_it_can() {
    // `_, ok := m[k]` is Go's one way to ask, and an `if` header is the only
    // place with room for a two-value read.
    let source = "\
def main() -> None:
    seen = set()
    seen.add(\"ada\")
    if \"ada\" in seen:
        print(\"yes\")
";
    let go = translated(source, "member.py", Language::Go);
    assert!(go.contains("if _, frOk := seen[\"ada\"]; frOk {"), "{go}");
}

#[test]
fn a_go_map_to_nothing_reads_back_as_the_set_it_is() {
    // `map[T]struct{}` carries no value, so membership is all it can answer.
    // `map[T]bool` is a map of booleans as much as it is a set, and a round
    // trip could not tell the two apart.
    let source = "\
package m

func main() {
\tseen := map[string]struct{}{}
\tseen[\"ada\"] = struct{}{}
\tif _, ok := seen[\"ada\"]; ok {
\t\tprintln(\"yes\")
\t}
}
";
    let python = translated(source, "seen.go", Language::Python);
    assert!(python.contains("seen = set()"), "{python}");
    assert!(python.contains("seen.add(\"ada\")"), "{python}");
    assert!(python.contains("if \"ada\" in seen:"), "{python}");
}

#[test]
fn a_value_the_source_reads_is_not_thrown_away() {
    // Rust's `HashSet::insert` answers whether the member was new, and no other
    // language's `add` answers anything. Renamed in a condition, the answer
    // went missing and Go wrote a statement where a value belonged.
    let source = "\
pub fn note(seen: &mut std::collections::HashSet<i64>, id: i64) -> bool {
    if seen.insert(id) {
        return true;
    }
    return false;
}
";
    let go = translated(source, "note.rs", Language::Go);
    assert!(
        !go.contains("if seen[id] = struct{}{} {"),
        "a store is not a condition:\n{go}"
    );
}
