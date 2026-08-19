//! Extract function: the data-flow half of the extract/inline family.
//!
//! What makes this refactoring hard is not moving the text but working out the
//! interface: which locals flow in as parameters, which flow out as returns, and
//! whether a call can reproduce the region's control flow at all.

use fun_refactor::edit::apply_to_string;
use fun_refactor::index::Index;
use fun_refactor::refactor::extract;
use fun_refactor::scan::{scan, ScanOptions};
use fun_refactor::span::{LineCol, LineIndex, Span};
use std::path::{Path, PathBuf};

fn workspace(files: &[(&str, &str)]) -> (tempfile::TempDir, Index) {
    let tmp = tempfile::tempdir().unwrap();
    for (name, content) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    (tmp, Index::build_from_scan(&scanned).unwrap())
}

/// Byte span of whole lines `from..=to` (1-based).
fn lines(source: &str, from: usize, to: usize) -> Span {
    let index = LineIndex::new(source);
    let start = index.line_span(from).unwrap().start;
    let end = index.line_span(to).unwrap().end;
    let _ = index.offset(LineCol { line: from, col: 1 }, source);
    Span::new(start, end)
}

fn apply(plan: &extract::ExtractFunctionPlan, path: &PathBuf) -> String {
    let original = std::fs::read_to_string(path).unwrap();
    apply_to_string(&original, plan.edits.edits_for(path).unwrap()).unwrap()
}

#[test]
fn extracts_statements_with_no_inputs_or_outputs() {
    let src = "def main():\n    print(\"one\")\n    print(\"two\")\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 2, 3), "greet").unwrap();
    assert!(plan.parameters.is_empty(), "got {:?}", plan.parameters);
    assert!(plan.returns.is_empty(), "got {:?}", plan.returns);

    let out = apply(&plan, &path);
    assert!(
        out.contains("    greet()"),
        "call replaces the region:\n{out}"
    );
    assert!(out.contains("def greet():"), "definition added:\n{out}");
    assert!(out.contains("print(\"one\")"), "body moved:\n{out}");
}

#[test]
fn locals_read_in_the_region_become_parameters() {
    let src =
        "def main():\n    width = 3\n    height = 4\n    area = width * height\n    print(area)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    // Extract only the `area = ...` line: it reads width and height.
    let plan = extract::function(&index, &path, lines(src, 4, 4), "compute").unwrap();
    let names: Vec<&str> = plan.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["height", "width"], "got {names:?}");
}

#[test]
fn locals_used_after_the_region_become_returns() {
    let src = "def main():\n    width = 3\n    area = width * 2\n    print(area)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "compute").unwrap();
    assert_eq!(plan.returns, vec!["area".to_string()]);

    let out = apply(&plan, &path);
    assert!(out.contains("area = compute(width)"), "got:\n{out}");
    assert!(out.contains("return area"), "got:\n{out}");
}

#[test]
fn a_local_not_used_afterwards_is_not_returned() {
    let src = "def main():\n    tmp = 1\n    print(tmp)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    // Both lines move, so `tmp` never escapes the new function.
    let plan = extract::function(&index, &path, lines(src, 2, 3), "work").unwrap();
    assert!(plan.returns.is_empty(), "got {:?}", plan.returns);
}

#[test]
fn comments_inside_the_region_survive() {
    // gopls is documented to drop comments here; reusing the original bytes keeps them.
    let src =
        "def main():\n    # explain the next line\n    x = compute()  # trailing\n    print(x)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 2, 3), "prepare").unwrap();
    let out = apply(&plan, &path);
    assert!(out.contains("# explain the next line"), "got:\n{out}");
    assert!(out.contains("# trailing"), "got:\n{out}");
}

#[test]
fn refuses_a_region_containing_a_return() {
    // A call cannot reproduce a return from the enclosing function.
    let src = "def main():\n    if bad():\n        return 1\n    print(2)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let err = extract::function(&index, &path, lines(src, 2, 3), "guard")
        .unwrap_err()
        .to_string();
    assert!(err.contains("return"), "got: {err}");
    assert!(err.contains("cannot"), "got: {err}");
}

#[test]
fn a_loop_carrying_its_own_break_is_extractable() {
    // `break` bound to a loop inside the region does not escape it.
    let src = "def main():\n    for i in items():\n        if done(i):\n            break\n    print(1)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 2, 4), "scan");
    assert!(
        plan.is_ok(),
        "should extract: {:?}",
        plan.err().map(|e| e.to_string())
    );
}

#[test]
fn refuses_a_name_already_in_use() {
    let src = "def existing():\n    pass\n\ndef main():\n    print(1)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let err = extract::function(&index, &path, lines(src, 5, 5), "existing").unwrap_err();
    assert!(
        err.downcast_ref::<fun_refactor::refactor::Refusal>()
            .is_some(),
        "got: {err}"
    );
}

#[test]
fn rust_copies_written_parameter_types() {
    let src = "fn main() {\n    let width: i32 = 3;\n    println!(\"{}\", width * 2);\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "show").unwrap();
    assert_eq!(plan.parameters.len(), 1);
    assert_eq!(plan.parameters[0].name, "width");
    assert_eq!(plan.parameters[0].type_annotation.as_deref(), Some("i32"));

    let out = apply(&plan, &path);
    assert!(out.contains("fn show(width: i32)"), "got:\n{out}");
}

#[test]
fn rust_refuses_when_a_parameter_type_was_never_written() {
    // There is no type inference here, and inventing one would not compile.
    let src = "fn main() {\n    let width = 3;\n    println!(\"{}\", width * 2);\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let err = extract::function(&index, &path, lines(src, 3, 3), "show")
        .unwrap_err()
        .to_string();
    assert!(err.contains("never written down"), "got: {err}");
    assert!(
        err.contains("width"),
        "the blocking name should be named: {err}"
    );
}

#[test]
fn refuses_unsupported_languages() {
    // Bash grew extract-function; HTML has nothing callable to extract into.
    let (tmp, index) = workspace(&[("page.html", "<div><p>hi</p></div>\n")]);
    let path = tmp.path().join("page.html");
    let err = extract::function(&index, &path, Span::new(0, 5), "f").unwrap_err();
    assert!(
        err.downcast_ref::<fun_refactor::refactor::Refusal>()
            .is_some_and(|r| matches!(r, fun_refactor::refactor::Refusal::Unsupported { .. })),
        "got: {err}"
    );
}

#[test]
fn the_result_still_parses() {
    let src = "def main():\n    width = 3\n    area = width * 2\n    print(area)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "compute").unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("an extraction must not break the file");
    assert_eq!(outcomes.len(), 1);
    let _ = Path::new(&path);
}

#[test]
fn typescript_extracts_without_type_annotations() {
    let src = "function main() {\n  const w = 3;\n  console.log(w * 2);\n}\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "show").unwrap();
    let out = apply(&plan, &path);
    assert!(out.contains("function show(w)"), "got:\n{out}");
    assert!(out.contains("show(w);"), "got:\n{out}");
}

#[test]
fn extracted_typescript_states_its_parameter_types_once() {
    // The C-family grammars fold the `:` into the annotation node, so reading the
    // type field verbatim and adding a `:` back produced `function helper(x: : number)`.
    let src = "function f(x: number) {\n  const a = x + 1;\n  use(a);\n}\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let plan = extract::function(&index, &path, lines(src, 2, 2), "helper").unwrap();
    let out = apply(&plan, &path);
    assert!(out.contains("function helper(x: number)"), "got:\n{out}");
    assert!(!out.contains(": :"), "doubled annotation:\n{out}");
    fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
        .expect("extracted TypeScript must parse");
}

#[test]
fn extracted_go_writes_go_and_not_c() {
    let src = "package p\n\nfunc f(x int) {\n\ta := x + 1\n\tuse(a)\n\tdone()\n}\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let plan = extract::function(&index, &path, lines(src, 4, 5), "helper").unwrap();
    let out = apply(&plan, &path);
    assert!(
        out.contains("func helper(x int)"),
        "colon-typed params:\n{out}"
    );
    assert!(
        !out.contains("helper(x);"),
        "Go has no statement semicolon:\n{out}"
    );
    assert!(
        out.contains("\tdone()\n}"),
        "`done()` should stay behind:\n{out}"
    );
}

#[test]
fn a_go_selection_takes_only_the_statements_selected() {
    // `statement_list` sits between a Go block and its statements; treating it as a
    // statement widened every selection to the whole block.
    let src = "package p\n\nfunc f(x int) {\n\ta := x + 1\n\tuse(a)\n\tdone()\n}\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let plan = extract::function(&index, &path, lines(src, 4, 5), "helper").unwrap();
    assert!(
        !plan.body.contains("done()"),
        "selection widened past what was asked:\n{}",
        plan.body
    );
}

#[test]
fn an_extracted_body_is_indented_the_way_the_file_is() {
    let src = "function f(x: number) {\n  const a = x + 1;\n  use(a);\n}\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let plan = extract::function(&index, &path, lines(src, 2, 3), "helper").unwrap();
    let out = apply(&plan, &path);
    assert!(
        out.contains("function helper(x: number) {\n  const a = x + 1;"),
        "four spaces in a two-space file:\n{out}"
    );
}

#[test]
fn extract_function_produces_parseable_code_in_every_imperative_language() {
    // Only Rust and Python were ever exercised here. TypeScript was broken.
    let cases: &[(&str, &str, usize, usize)] = &[
        (
            "a.rs",
            "fn f(x: i32) {\n    let a: i32 = x + 1;\n    use_it(a);\n}\n",
            2,
            2,
        ),
        (
            "a.go",
            "package p\n\nfunc f(x int) {\n\tvar a int = x + 1\n\tuse(a)\n}\n",
            4,
            4,
        ),
        (
            "a.zig",
            "fn f(x: i32) void {\n    const a: i32 = x + 1;\n    use(a);\n}\n",
            2,
            2,
        ),
        (
            "a.ts",
            "function f(x: number) {\n  const a = x + 1;\n  use(a);\n}\n",
            2,
            2,
        ),
        (
            "a.tsx",
            "function f(x: number) {\n  const a = x + 1;\n  use(a);\n}\n",
            2,
            2,
        ),
        ("a.py", "def f(x):\n    a = x + 1\n    use(a)\n", 2, 2),
    ];
    for (name, src, from, to) in cases {
        let (tmp, index) = workspace(&[(name, src)]);
        let path = tmp.path().join(name);
        let plan = extract::function(&index, &path, lines(src, *from, *to), "helper")
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .unwrap_or_else(|e| panic!("{name} produced code that will not parse: {e}"));
    }
}

#[test]
fn a_binding_mutated_in_the_region_travels_back_as_a_return() {
    // `total` is declared before the region, assigned inside it, and read after.
    // Passed by value alone, the mutation dies with the call and the function
    // quietly computes zero.
    let src = "def tally(items):\n    total = 0\n    for item in items:\n        \
               total += item\n    return total\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let plan = extract::function(&index, &path, lines(src, 3, 4), "add_up").unwrap();
    assert_eq!(
        plan.returns,
        vec!["total".to_string()],
        "{:?}",
        plan.returns
    );

    let out = apply(&plan, &path);
    assert!(
        out.contains("    total = add_up(items, total)"),
        "the changed value is assigned back.\n{out}"
    );
    assert!(
        out.contains("def add_up(items, total):"),
        "the binding still flows in:\n{out}"
    );
    assert!(
        out.contains("    return total"),
        "the helper returns what it changed.\n{out}"
    );
}

#[test]
fn a_mutated_rust_binding_is_reassigned_and_the_parameter_is_mut() {
    let src = "fn tally(items: &[i64]) -> i64 {\n    let mut total: i64 = 0;\n    \
               for item in items {\n        total += item;\n    }\n    total\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let plan = extract::function(&index, &path, lines(src, 3, 5), "add_up").unwrap();
    let out = apply(&plan, &path);
    assert!(
        out.contains("    total = add_up(items, total);"),
        "assigned, never shadowed with a fresh `let`:\n{out}"
    );
    assert!(
        out.contains("mut total: i64) -> i64"),
        "a parameter the body assigns must say `mut`:\n{out}"
    );
}

#[test]
fn a_typescript_assignment_target_is_a_use_of_the_binding() {
    // `total += item` was invisible to the index. The region moved without
    // `total` flowing in, and the output named a binding that no longer existed.
    let src = "export function tally(items: number[]): number {\n    \
               let total: number = 0;\n    for (const item of items) {\n        \
               total += item;\n    }\n    return total;\n}\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let plan = extract::function(&index, &path, lines(src, 3, 5), "addUp").unwrap();
    assert_eq!(
        plan.parameters
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        vec!["items", "total"],
        "{:?}",
        plan.parameters
    );
    let out = apply(&plan, &path);
    assert!(
        out.contains("    total = addUp(items, total);"),
        "got:\n{out}"
    );
}

#[test]
fn zig_refuses_a_region_that_assigns_an_outside_binding() {
    // A Zig parameter cannot be assigned, so the changed value has no way back.
    let src = "fn tally(items: []const i64) i64 {\n    var total: i64 = 0;\n    \
               for (items) |item| {\n        total += item;\n    }\n    return total;\n}\n";
    let (tmp, index) = workspace(&[("a.zig", src)]);
    let path = tmp.path().join("a.zig");

    let err = extract::function(&index, &path, lines(src, 3, 5), "addUp")
        .unwrap_err()
        .to_string();
    assert!(err.contains("cannot be assigned"), "got: {err}");
    assert!(
        err.contains("total"),
        "the refusal names the binding: {err}"
    );
}

#[test]
fn a_statement_range_without_function_is_refused_by_name() {
    let src = "def tally(items):\n    total = 0\n    for item in items:\n        \
               total += item\n    return total\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let start = src.find("for item").unwrap();
    let end = src.find("total += item").unwrap() + "total += item".len();
    let err = extract::variable(&index, &path, Span::new(start, end), "loop", false)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("--function"),
        "the refusal points at the flag that does this. {err}"
    );
}

#[test]
fn java_extracts_with_types_copied_and_the_mutation_carried_back() {
    let src = "public final class Calc {\n    static int tally(int[] items) { // Sums.\n        \
               int total = 0;\n        for (int item : items) {\n            \
               total += item;\n        }\n        return total; // The sum.\n    }\n}\n";
    let (tmp, index) = workspace(&[("Calc.java", src)]);
    let path = tmp.path().join("Calc.java");

    let plan = extract::function(&index, &path, lines(src, 4, 6), "addUp").unwrap();
    let out = apply(&plan, &path);
    assert!(
        out.contains("total = addUp(items, total);"),
        "the changed value is assigned back, never re-declared.\n{out}"
    );
    assert!(
        out.contains("    static int addUp(int[] items, int total) {"),
        "types copied, member indent kept:\n{out}"
    );
}

#[test]
fn java_recovers_the_type_a_var_local_was_given() {
    // `var` is the word for "the compiler worked it out", and in Java it works it out
    // from the initializer alone. So `var n = 2` is an `int` and the parameter says so.
    let src = "public final class Calc {\n    static int twice() { // Doubles.\n        \
               var n = 2;\n        int m = n * 2;\n        return m; // Done.\n    }\n}\n";
    let (tmp, index) = workspace(&[("Calc.java", src)]);
    let path = tmp.path().join("Calc.java");

    let plan = extract::function(&index, &path, lines(src, 4, 4), "doubled").unwrap();
    let out = apply(&plan, &path);
    assert!(out.contains("static int doubled(int n)"), "got:\n{out}");
}

#[test]
fn java_refuses_when_a_local_type_is_beyond_reach() {
    // Nothing states what `n` holds and nothing derives it, so there is no type to copy.
    let src =
        "public final class Calc {\n    static int twice(java.util.List<Integer> xs) {\n        \
               var n = xs.get(0);\n        int m = n * 2;\n        return m;\n    }\n}\n";
    let (tmp, index) = workspace(&[("Calc.java", src)]);
    let path = tmp.path().join("Calc.java");

    let err = extract::function(&index, &path, lines(src, 4, 4), "doubled")
        .unwrap_err()
        .to_string();
    assert!(err.contains("never written down"), "got: {err}");
    assert!(err.contains("n"), "the blocking name is named: {err}");
}

#[test]
fn go_returns_two_values_as_a_go_result_list() {
    // Two live-out values are Go's own multi-value return, and the signature has to
    // spell them as a parenthesised list or the file does not compile.
    let src = "package main\n\nfunc process() int {\n\tvar total int = 6\n\t\
               var count int = 3\n\n\tvar avg int = total / count\n\tvar scaled int = avg * 2\n\n\t\
               return avg + scaled + count\n}\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let plan = extract::function(&index, &path, lines(src, 7, 8), "computeStats").unwrap();
    assert_eq!(
        plan.returns,
        vec!["avg", "scaled"],
        "got {:?}",
        plan.returns
    );
    let out = apply(&plan, &path);
    assert!(
        out.contains("func computeStats(count int, total int) (int, int)"),
        "got:\n{out}"
    );
    assert!(out.contains("return avg, scaled"), "got:\n{out}");
}

#[test]
fn go_derives_the_type_a_short_declaration_gave() {
    // `total := 0` is an `int` by Go's own rule, so an idiomatic file extracts without
    // being annotated first.
    let src = "package main\n\nfunc process() int {\n\ttotal := 6\n\t\
               count := 3\n\n\tavg := total / count\n\tscaled := avg * 2\n\n\t\
               return avg + scaled + count\n}\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let plan = extract::function(&index, &path, lines(src, 7, 8), "computeStats").unwrap();
    let out = apply(&plan, &path);
    assert!(
        out.contains("func computeStats(count int, total int) (int, int)"),
        "got:\n{out}"
    );
}

#[test]
fn go_refuses_a_return_whose_type_is_beyond_reach() {
    // An untyped value from outside the workspace derives nothing, and a Go signature
    // cannot be written without one.
    let src = "package main\n\nimport \"os\"\n\nfunc process() string {\n\t\
               host := os.Getenv(\"HOST\")\n\tport := os.Getenv(\"PORT\")\n\t\
               return host + port\n}\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let err = extract::function(&index, &path, lines(src, 6, 7), "readEnv")
        .unwrap_err()
        .to_string();
    assert!(err.contains("host"), "the blocking name is named: {err}");
}

#[test]
fn java_extracts_an_expression_into_a_var_binding() {
    let src = "public final class Calc {\n    static int twice(int n) {\n        \
               return n * 2 + 1;\n    }\n}\n";
    let (tmp, index) = workspace(&[("Calc.java", src)]);
    let path = tmp.path().join("Calc.java");

    let start = src.find("n * 2").unwrap();
    let plan =
        extract::variable(&index, &path, Span::new(start, start + 5), "doubled", false).unwrap();
    let out = apply2(&plan, &path);
    assert!(
        out.contains("var doubled = n * 2;"),
        "`var` infers the way `let` does:\n{out}"
    );
    assert!(out.contains("return doubled + 1;"), "got:\n{out}");
}

fn apply2(plan: &extract::ExtractPlan, path: &PathBuf) -> String {
    let original = std::fs::read_to_string(path).unwrap();
    apply_to_string(&original, plan.edits.edits_for(path).unwrap()).unwrap()
}

#[test]
fn a_selection_crossing_a_loop_body_is_refused_with_the_boundary_named() {
    // One end inside the loop's body and the other outside it. The moved
    // statements keep their bytes, so the extraction carried the loop's own
    // outdent into the middle of the new function. It wrote a file that does
    // not parse, and reported success.
    let src = "def f(items):\n    total = 0\n    for x in items:\n        total += x\n    \
        result = total * 10\n    return result\n";
    let (tmp, index) = workspace(&[("loop.py", src)]);
    let path = tmp.path().join("loop.py");
    let err = extract::function(&index, &path, lines(src, 4, 5), "helper")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("crosses the body of the `for`"),
        "the refusal names the boundary: {err}"
    );
}

#[test]
fn the_same_guard_holds_in_typescript() {
    let src = "export function f(items: number[]): number {\n  let total = 0;\n  \
        for (const x of items) {\n    // Accumulate.\n    total += x;\n  }\n  \
        const result = total * 10;\n  return result;\n}\n";
    let (tmp, index) = workspace(&[("loop.ts", src)]);
    let path = tmp.path().join("loop.ts");
    let err = extract::function(&index, &path, lines(src, 5, 7), "helper")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("crosses the body of the `for`"),
        "the guard is shared, not per language: {err}"
    );
}

#[test]
fn a_selection_wholly_inside_a_loop_body_still_extracts() {
    let src = "def f(items):\n    total = 0\n    for x in items:\n        total += x\n        \
        total += 1\n    return total\n";
    let (tmp, index) = workspace(&[("loop.py", src)]);
    let path = tmp.path().join("loop.py");
    let plan = extract::function(&index, &path, lines(src, 4, 5), "step").expect("a plan");
    let out = apply(&plan, &path);
    assert!(out.contains("def step("), "the extraction happened.\n{out}");
}

// --------------------------------------------------- where the definition lands

#[test]
fn extracting_from_a_method_that_is_not_the_last_keeps_the_class_whole() {
    // The definition used to go straight after the method it came from, at column
    // zero. Python nests a `def` anywhere, so the file still parsed and every method
    // below became a closure of the new function. `Box` lost `total`, and the tests
    // that called it failed with an `AttributeError`.
    let src = "class Box:\n    def __init__(self):\n        self.values = []\n\n    \
        def add(self, v):\n        self.values.append(v * 2)\n        return self\n\n    \
        def total(self):\n        return sum(self.values)\n";
    let (tmp, index) = workspace(&[("box.py", src)]);
    let path = tmp.path().join("box.py");

    let plan = extract::function(&index, &path, lines(src, 6, 6), "record").expect("a plan");
    let out = apply(&plan, &path);
    let definition = out.find("\ndef record(").unwrap_or_else(|| {
        panic!("the definition is hoisted out of the class:\n{out}");
    });
    let total = out.find("    def total(self):").expect("total survives");
    assert!(
        total < definition,
        "the class keeps every method it had:\n{out}"
    );
    assert!(out.contains("        record(self, v)"), "the call:\n{out}");
}

#[test]
fn extracting_from_the_last_method_lands_in_the_same_place() {
    // The last method was the one case that worked, because there was nothing below
    // it to swallow. It has to keep working, and land where the other one does.
    let src = "class Box:\n    def __init__(self):\n        self.values = []\n\n    \
        def total(self):\n        return sum(self.values)\n";
    let (tmp, index) = workspace(&[("box.py", src)]);
    let path = tmp.path().join("box.py");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "start").expect("a plan");
    let out = apply(&plan, &path);
    assert!(
        out.contains("\ndef start("),
        "hoisted out of the class:\n{out}"
    );
    assert!(out.contains("    def total(self):"), "class intact:\n{out}");
}

#[test]
fn extracting_from_a_nested_function_stays_inside_the_outer_one() {
    // Hoisting stops at a function. The new definition reads `base`, which belongs to
    // `outer`, so moving it to the top of the file would leave that name undefined.
    let src = "def outer(base):\n    def inner(v):\n        scaled = v * base\n        \
        return scaled + 1\n\n    return inner(2)\n";
    let (tmp, index) = workspace(&[("n.py", src)]);
    let path = tmp.path().join("n.py");

    let plan = extract::function(&index, &path, lines(src, 3, 3), "scale").expect("a plan");
    let out = apply(&plan, &path);
    assert!(
        out.contains("\n    def scale(v):"),
        "written at the inner function's own indentation:\n{out}"
    );
    assert!(
        out.trim_end().ends_with("return inner(2)"),
        "the outer body is not swallowed:\n{out}"
    );
}

#[test]
fn a_typescript_method_carries_its_receiver_as_a_parameter() {
    // `this` is named nowhere in a TypeScript signature, so the data-flow analysis
    // could not see it. The body still said `this.values` while the parameter list
    // was empty. Go reaches the same shape by ordinary means, its receiver being a
    // named parameter, and this follows it.
    let src = "export class Box {\n  values: number[] = [];\n\n  sum(): number {\n    \
        let running = 0;\n    for (const v of this.values) {\n      running = running + v;\n    \
        }\n    return running;\n  }\n}\n";
    let (tmp, index) = workspace(&[("m.ts", src)]);
    let path = tmp.path().join("m.ts");

    let plan = extract::function(&index, &path, lines(src, 5, 8), "accumulate").expect("a plan");
    let names: Vec<&str> = plan.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["self"],
        "the receiver leads the list: {names:?}"
    );

    let out = apply(&plan, &path);
    assert!(
        out.contains("function accumulate(self: Box)"),
        "typed with the class it came out of:\n{out}"
    );
    assert!(out.contains("of self.values"), "`this` rewritten:\n{out}");
    assert!(
        out.contains("accumulate(this)"),
        "the call passes it:\n{out}"
    );
    assert!(
        !out.contains("  function accumulate"),
        "hoisted clear of the class body:\n{out}"
    );
}

#[test]
fn a_private_member_stops_the_extraction_leaving_the_class() {
    // `private` is checked by the compiler, so a function outside the class that
    // reads one is code that runs and does not build.
    let src = "export class Box {\n  private values: number[] = [];\n\n  sum(): number {\n    \
        let running = 0;\n    for (const v of this.values) {\n      running = running + v;\n    \
        }\n    return running;\n  }\n}\n";
    let (tmp, index) = workspace(&[("m.ts", src)]);
    let path = tmp.path().join("m.ts");

    let err = extract::function(&index, &path, lines(src, 5, 8), "accumulate")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("this.values") && err.contains("does not expose"),
        "the refusal names the member: {err}"
    );
}

#[test]
fn a_rust_method_that_reads_self_is_refused() {
    // The definition leaves the `impl` block, so it is a free function with no
    // receiver. Rust cannot be handed one as an ordinary parameter without inventing
    // a borrow and a lifetime, so the extraction says so instead of guessing.
    let src = "pub struct Box2 {\n    pub values: Vec<i64>,\n}\n\nimpl Box2 {\n    \
        pub fn add(&mut self, v: i64) {\n        let doubled = v * 2;\n        \
        self.values.push(doubled);\n    }\n}\n";
    let (tmp, index) = workspace(&[("lib.rs", src)]);
    let path = tmp.path().join("lib.rs");

    let err = extract::function(&index, &path, lines(src, 7, 8), "helper")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("reads `self`") && err.contains("no receiver"),
        "the refusal names the receiver: {err}"
    );
}
