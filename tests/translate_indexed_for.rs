//! The element beside its position crosses every boundary here.

mod common;

use fun_refactor::lang::Language;
use fun_refactor::transpile;

const IDX_ZIG: &str = "fn tally(xs: []const i64) i64 {\n    var total: i64 = 0;\n    \
                       for (xs, 0..) |x, i| {\n        total = total + x * i;\n    }\n    \
                       return total;\n}\n";

#[test]
fn a_zig_counted_for_crosses_into_every_target() {
    let (_tmp, root) = common::tree(&[("idx.zig", IDX_ZIG)]);
    let cases = [
        (Language::Python, "for i, x in enumerate(xs):"),
        (Language::Go, "for i, x := range xs {"),
        (Language::Rust, "for (i, x) in xs.iter().enumerate() {"),
        (Language::TypeScript, "let i = 0;"),
        (Language::TypeScript, "i += 1;"),
        (Language::Java, "int i = 0;"),
        (Language::Java, "for (var x : xs) {"),
    ];
    for (to, expected) in cases {
        let plan = transpile::plan(&root.join("idx.zig"), to).expect("a draft");
        assert!(
            plan.output.contains(expected),
            "{to} is missing `{expected}`:\n{}",
            plan.output
        );
        assert!(
            !plan.output.contains(transpile::MARKER),
            "{to} carried what it can say:\n{}",
            plan.output
        );
    }
}

#[test]
fn a_python_enumerate_reads_and_zig_writes_it_back() {
    let source = "def tally(xs: list[int]) -> int:\n    total = 0\n    \
                  for i, x in enumerate(xs):\n        total = total + x * i\n    return total\n";
    let (_tmp, root) = common::tree(&[("en.py", source)]);
    let plan = transpile::plan(&root.join("en.py"), Language::Zig).expect("a draft");
    assert!(
        plan.output.contains("for (xs, 0..) |x, i| {"),
        "{}",
        plan.output
    );
}

#[test]
fn two_real_sequences_walk_by_one_index() {
    let source = "fn pair(xs: []const i64, ys: []const i64) void {\n    \
                  for (xs, ys) |x, y| {\n        use(x, y);\n    }\n}\n";
    let (_tmp, root) = common::tree(&[("z.zig", source)]);
    let plan = transpile::plan(&root.join("z.zig"), Language::Python).expect("a draft");
    assert!(
        plan.output.contains("for fr_i, x in enumerate(xs):")
            && plan.output.contains("y = ys[fr_i]"),
        "the second sequence reads by the shared index:\n{}",
        plan.output
    );
    assert!(
        !plan.output.contains(transpile::MARKER),
        "nothing carries:\n{}",
        plan.output
    );
}
