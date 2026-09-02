//! Inline call: the strictest refactoring in the set.

use fun_refactor::edit::apply_to_string;
use fun_refactor::refactor::inline;
use std::path::PathBuf;

mod common;
use common::workspace;

fn apply(plan: &inline::InlineCallPlan, path: &PathBuf) -> String {
    let original = std::fs::read_to_string(path).unwrap();
    apply_to_string(&original, plan.edits.edits_for(path).unwrap()).unwrap()
}

#[test]
fn inlines_a_thin_wrapper() {
    let src = "fn double(x: i32) -> i32 { x * 2 }\nfn main() { let y = double(3); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("double").unwrap() + 1;
    let plan = inline::call(&index, &path, at).unwrap();
    assert_eq!(plan.function, "double");

    let out = apply(&plan, &path);
    assert!(out.contains("let y = 3 * 2;"), "got:\n{out}");
    // The definition stays: inlining one call deletes no function.
    assert!(out.contains("fn double(x: i32)"), "got:\n{out}");
}

#[test]
fn substitutes_each_argument_for_its_parameter() {
    let src = "fn add(a: i32, b: i32) -> i32 { a + b }\nfn main() { let s = add(p, q); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("add").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("let s = p + q;"), "got:\n{out}");
}

#[test]
fn substitutes_a_rust_method_receiver_for_self() {
    let src = "struct Scale { amount: i32 }\n\nimpl Scale {\n    fn plus(&self, n: i32) -> i32 { self.amount + n }\n    fn run(&self) -> i32 { self.plus(3) }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("plus").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(
        out.contains("fn run(&self) -> i32 { self.amount + 3 }"),
        "got:\n{out}"
    );
}

#[test]
fn substitutes_a_mutable_rust_method_receiver_for_self() {
    let src = "struct Scale { amount: i32 }\n\nimpl Scale {\n    fn slot(&mut self) -> &mut i32 { &mut self.amount }\n    fn run(&mut self) -> &mut i32 { self.slot() }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("slot").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(
        out.contains("fn run(&mut self) -> &mut i32 { &mut self.amount }"),
        "got:\n{out}"
    );
}

#[test]
fn substitutes_a_typed_rust_method_receiver_for_self() {
    let src = "struct Scale { amount: i32 }\n\nimpl Scale {\n    fn take(self: Box<Self>) -> i32 { self.amount }\n    fn run(self: Box<Self>) -> i32 { self.take() }\n}\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("take").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(
        out.contains("fn run(self: Box<Self>) -> i32 { self.amount }"),
        "got:\n{out}"
    );
}

#[test]
fn refuses_to_substitute_through_a_closures_parameter() {
    let src = "fn make_offset(x: i32) -> impl Fn(i32) -> i32 { |x| x + 1 }\nfn main() { let offset = make_offset(3); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("make_offset").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("binds parameter 'x'"), "got: {err}");
}

#[test]
fn refuses_to_substitute_through_an_arrow_functions_parameter() {
    let src = "function makeOffset(x: number): (x: number) => number { return x => x + 1; }\nfunction main() { const offset = makeOffset(3); }\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let at = src.rfind("makeOffset").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("binds parameter 'x'"), "got: {err}");
}

#[test]
fn refuses_to_expand_a_rust_struct_shorthand() {
    let src = "struct Point { x: i32 }\nfn point(x: i32) -> Point { Point { x } }\nfn main() { let p = point(3); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("point").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("shorthand"), "got: {err}");
}

#[test]
fn refuses_to_expand_a_typescript_object_shorthand() {
    let src =
        "function point(x: number) { return { x }; }\nfunction main() { const p = point(3); }\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let at = src.rfind("point").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("shorthand"), "got: {err}");
}

#[test]
fn refuses_to_substitute_a_rust_field_name() {
    let src = "struct Point { x: i32 }\nfn read(x: i32, point: Point) -> i32 { point.x }\nfn main() { let point = Point { x: 1 }; let y = read(3, point); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("read").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("field name"), "got: {err}");
}

#[test]
fn refuses_to_substitute_a_typescript_member_name() {
    let src = concat!(
        "type Point = { x: number };\n",
        "function read(x: number, point: Point): number { return point.x; }\n",
        "function main() { const point = { x: 1 }; const y = read(3, point); }\n",
    );
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let at = src.rfind("read").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("field name"), "got: {err}");
}

#[test]
fn preserves_a_rust_string_literal_while_inlining() {
    let src = concat!(
        "fn label(x: i32) -> &'static str { \"x\" }\n",
        "fn main() { let label_text = label(3); }\n",
    );
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("label").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("let label_text = \"x\";"), "got:\n{out}");
}

#[test]
fn preserves_a_rust_character_literal_while_inlining() {
    let src = concat!(
        "fn label(x: char) -> char { 'x' }\n",
        "fn main() { let label_char = label('a'); }\n",
    );
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("label").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("let label_char = 'x';"), "got:\n{out}");
}

#[test]
fn preserves_a_typescript_string_literal_while_inlining() {
    let src = concat!(
        "function label(x: number): string { return \"x\"; }\n",
        "function main() { const labelText = label(3); }\n",
    );
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let at = src.rfind("label").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("const labelText = \"x\";"), "got:\n{out}");
}

#[test]
fn preserves_a_comment_and_allows_its_single_effectful_use() {
    let src = concat!(
        "fn add_one(x: i32) -> i32 { x /* x stays prose */ + 1 }\n",
        "fn next() -> i32 { 3 }\n",
        "fn main() { let total = add_one(next()); }\n",
    );
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("add_one").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(
        out.contains("let total = next() /* x stays prose */ + 1;"),
        "got:\n{out}"
    );
}

#[test]
fn preserves_a_typescript_regular_expression_while_inlining() {
    let src = concat!(
        "function matches(x: string, value: string): boolean { return /x/.test(value); }\n",
        "function main(input: string) { const matched = matches(\"a\", input); }\n",
    );
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let at = src.rfind("matches").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(
        out.contains("const matched = /x/.test(input);"),
        "got:\n{out}"
    );
}

#[test]
fn refuses_to_substitute_a_python_keyword_argument() {
    let src = "def double(x):\n    return x * 2\n\ndef main():\n    y = double(x=3)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let at = src.rfind("double").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("keyword"), "got: {err}");
}

#[test]
fn refuses_to_substitute_a_python_expanded_argument() {
    let src = "def double(x):\n    return x * 2\n\ndef main(values):\n    y = double(*values)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let at = src.rfind("double").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("expanded"), "got: {err}");
}

#[test]
fn substitutes_a_typed_python_parameter() {
    let src =
        "def double(x: int) -> int:\n    return x * 2\n\ndef main() -> None:\n    y = double(3)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("y = 3 * 2"), "got:\n{out}");
}

#[test]
fn refuses_to_substitute_a_destructured_parameter() {
    let src = concat!(
        "function double({ value }: { value: number }): number { return value * 2; }\n",
        "function main() { const y = double({ value: 3 }); }\n",
    );
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let at = src.rfind("double").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("simple name"), "got: {err}");
}

#[test]
fn parenthesises_so_precedence_is_preserved() {
    // Without brackets `2 + one() * 3` would re-associate and change the answer.
    let src = "fn sum() -> i32 { 1 + 1 }\nfn main() { let n = 2 * sum(); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("sum").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("2 * (1 + 1)"), "got:\n{out}");
}

#[test]
fn a_bare_name_expansion_is_not_parenthesised() {
    let src = "fn get(v: i32) -> i32 { v }\nfn main() { let n = get(x); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("get").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("let n = x;"), "got:\n{out}");
}

#[test]
fn refuses_a_multi_statement_body() {
    let src = "fn work(x: i32) -> i32 {\n    let t = x + 1;\n    t * 2\n}\nfn main() { let y = work(3); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("work").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("several statements"), "got: {err}");
}

#[test]
fn refuses_to_duplicate_an_effectful_argument() {
    // `x` appears twice in the body, so passing a call would run it twice.
    let src = "fn square(x: i32) -> i32 { x * x }\nfn main() { let y = square(next()); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("square").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("more than once"), "got: {err}");
}

#[test]
fn a_repeated_parameter_with_a_plain_argument_is_fine() {
    let src = "fn square(x: i32) -> i32 { x * x }\nfn main() { let y = square(n); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("square").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("let y = n * n;"), "got:\n{out}");
}

#[test]
fn refuses_when_the_argument_count_disagrees() {
    let src = "fn two(a: i32, b: i32) -> i32 { a + b }\nfn main() { let y = two(1); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("two").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("parameter"), "got: {err}");
}

#[test]
fn refuses_a_recursive_call() {
    let src = "fn loops(n: i32) -> i32 { loops(n) }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("loops").unwrap() + 1;
    let err = inline::call(&index, &path, at).unwrap_err().to_string();
    assert!(err.contains("not terminate"), "got: {err}");
}

#[test]
fn refuses_when_the_position_is_not_a_call() {
    let src = "fn f() -> i32 { 1 }\nfn main() { let y = 2; }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.find("let y").unwrap() + 4;
    assert!(inline::call(&index, &path, at).is_err());
}

#[test]
fn works_for_python() {
    let src = "def double(x):\n    return x * 2\n\ndef main():\n    y = double(3)\n";
    let (tmp, index) = workspace(&[("a.py", src)]);
    let path = tmp.path().join("a.py");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("y = 3 * 2"), "got:\n{out}");
}

#[test]
fn the_result_still_parses() {
    let src = "fn double(x: i32) -> i32 { x * 2 }\nfn main() { let y = double(3); }\n";
    let (tmp, index) = workspace(&[("a.rs", src)]);
    let path = tmp.path().join("a.rs");

    let at = src.rfind("double").unwrap() + 1;
    let plan = inline::call(&index, &path, at).unwrap();
    let outcomes =
        fun_refactor::edit::plan(&plan.edits, fun_refactor::edit::Validation::ReparseStrict)
            .expect("inlining must not break the file");
    assert_eq!(outcomes.len(), 1);
}

#[test]
fn works_for_go_despite_its_statement_list_wrapper() {
    let src = "package main\n\nfunc double(x int) int { return x * 2 }\n\nfunc main() { y := double(3); _ = y }\n";
    let (tmp, index) = workspace(&[("a.go", src)]);
    let path = tmp.path().join("a.go");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("y := 3 * 2"), "got:\n{out}");
}

#[test]
fn works_for_typescript() {
    let src =
        "function double(x: number) { return x * 2; }\nfunction main() { const y = double(3); }\n";
    let (tmp, index) = workspace(&[("a.ts", src)]);
    let path = tmp.path().join("a.ts");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("const y = 3 * 2;"), "got:\n{out}");
}

#[test]
fn works_for_zig() {
    let src = "fn double(x: i32) i32 { return x * 2; }\npub fn main() void { const y = double(3); _ = y; }\n";
    let (tmp, index) = workspace(&[("a.zig", src)]);
    let path = tmp.path().join("a.zig");

    let at = src.rfind("double").unwrap() + 1;
    let out = apply(&inline::call(&index, &path, at).unwrap(), &path);
    assert!(out.contains("const y = 3 * 2;"), "got:\n{out}");
}

#[test]
fn a_callee_reading_its_own_modules_global_refuses_to_cross_files() {
    // `clamp` reads `LIMIT` from beside itself.
    let (tmp, index) = workspace(&[
        (
            "lib.py",
            "LIMIT = 10\n\n\ndef clamp(x: int) -> int:\n    return min(x, LIMIT)\n",
        ),
        (
            "app.py",
            "from lib import clamp\n\n\ndef run(v: int) -> int:\n    return clamp(v)\n",
        ),
    ]);
    let source = std::fs::read_to_string(tmp.path().join("app.py")).unwrap();
    let offset = source.find("clamp(v)").unwrap();
    let err = inline::call(&index, &tmp.path().join("app.py"), offset).unwrap_err();
    assert!(
        err.to_string().contains("LIMIT"),
        "the refusal names the carried global: {err}"
    );
}

#[test]
fn a_callee_reading_its_own_modules_import_refuses_to_cross_files() {
    // `home_dir` reads `os` from its own file's imports.
    let (tmp, index) = workspace(&[
        (
            "helpers.py",
            "import os\n\n\ndef home_dir() -> str:\n    return os.environ[\"HOME\"]\n",
        ),
        (
            "app.py",
            "from helpers import home_dir\n\n\ndef whereami() -> str:\n    return home_dir()\n",
        ),
    ]);
    let source = std::fs::read_to_string(tmp.path().join("app.py")).unwrap();
    let offset = source.find("home_dir()").unwrap();
    let err = inline::call(&index, &tmp.path().join("app.py"), offset).unwrap_err();
    assert!(
        err.to_string().contains("`os`"),
        "the refusal names the carried import: {err}"
    );
}

#[test]
fn a_callee_import_also_present_at_the_call_site_inlines() {
    // Both files import `os`, so the pasted body reads the same module either
    // way and the inline goes through.
    let (tmp, index) = workspace(&[
        (
            "helpers.py",
            "import os\n\n\ndef home_dir() -> str:\n    return os.environ[\"HOME\"]\n",
        ),
        (
            "app.py",
            "import os\n\nfrom helpers import home_dir\n\n\ndef whereami() -> str:\n    return home_dir()\n",
        ),
    ]);
    let source = std::fs::read_to_string(tmp.path().join("app.py")).unwrap();
    let offset = source.find("home_dir()").unwrap();
    let path = tmp.path().join("app.py");
    let out = apply(&inline::call(&index, &path, offset).unwrap(), &path);
    assert!(out.contains("return os.environ[\"HOME\"]"), "got:\n{out}");
}

#[test]
fn what_extract_function_writes_is_refused_and_the_refusal_says_so() {
    // The two were documented as a pair whose intersection is empty.
    let src = "def total(items):\n    running = accumulate(items)\n    return running\n\n\
        def accumulate(items):\n    running = 0\n    for i in items:\n        \
        running = running + i\n    return running\n";
    let (tmp, index) = workspace(&[("m.py", src)]);
    let path = tmp.path().join("m.py");

    let offset = src.find("accumulate(items)").unwrap();
    let err = inline::call(&index, &path, offset).unwrap_err().to_string();
    assert!(
        err.contains("several statements") && err.contains("one expression"),
        "the refusal names the limit: {err}"
    );
    assert!(
        err.contains("fr extract --function"),
        "and names the command whose output cannot come back: {err}"
    );
}

#[test]
fn the_documented_pairing_is_the_one_that_holds() {
    // `fr extract --variable` writes a binding of one expression, and inlining that
    // binding's function form is the round trip the docs promise.
    let src = "def price():\n    return base() * 2\n\n\ndef base():\n    return 5\n";
    let (tmp, index) = workspace(&[("m.py", src)]);
    let path = tmp.path().join("m.py");

    let offset = src.find("base() * 2").unwrap();
    let out = apply(&inline::call(&index, &path, offset).unwrap(), &path);
    assert!(out.contains("return 5 * 2"), "got:\n{out}");
}
