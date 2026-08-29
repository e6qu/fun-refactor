//! Does the code that a refactoring writes still compile?

mod common;
use common::{
    awkward, gate, must_plan, must_refuse, plain, rustc_is_available, the_free_function, Toolchain,
    Workspace,
};

use fun_refactor::edit::EditSet;
use fun_refactor::index::Index;
use std::process::Command;

#[test]
fn organising_imports_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let planned =
        fun_refactor::refactor::imports::plan(&index, &ws.path("src/util.rs")).map(|p| p.edits);
    must_plan("organising imports", &ws, planned);
    assert!(
        ws.read("src/util.rs")
            .contains("#[cfg(feature = \"extra\")]\nuse crate::extra::Extra;"),
        "the attribute lost its import:\n{}",
        ws.read("src/util.rs")
    );
}

#[test]
fn moving_a_symbol_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("src/util.rs"))
        .map(|p| p.edits);
    must_plan("moving a symbol", &ws, planned);
    assert!(
        ws.read("tests/it.rs").contains("gate_plain::util::width"),
        "the integration test was not repointed:\n{}",
        ws.read("tests/it.rs")
    );
}

#[test]
fn changing_a_signature_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    must_plan("changing a signature", &ws, planned);
}

#[test]
fn renaming_a_function_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "span_of").map(|p| p.edits);
    must_plan("renaming a function", &ws, planned);
}

#[test]
fn inlining_a_variable_compiles() {
    let ws = Workspace::new(&plain());
    let index = ws.index();
    let total = index
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .expect("the local")
        .id;
    let planned = fun_refactor::refactor::inline::variable(&index, total).map(|p| p.edits);
    must_plan("inlining a variable", &ws, planned);
}

/// Here a plan is optional.
#[test]
fn no_command_writes_a_broken_workspace() {
    let mut refused = Vec::new();
    for (what, plan) in [
        (
            "rename",
            Box::new(|ws: &Workspace, index: &Index| {
                let _ = ws;
                fun_refactor::refactor::rename::plan(
                    index,
                    the_free_function(index, "width"),
                    "span_of",
                )
                .map(|p| p.edits)
            }) as Box<dyn Fn(&Workspace, &Index) -> anyhow::Result<EditSet>>,
        ),
        (
            "move",
            Box::new(|ws: &Workspace, index: &Index| {
                fun_refactor::refactor::move_symbol::to_file(
                    index,
                    the_free_function(index, "width"),
                    &ws.path("src/util.rs"),
                )
                .map(|p| p.edits)
            }),
        ),
        (
            "signature",
            Box::new(|ws: &Workspace, index: &Index| {
                let _ = ws;
                fun_refactor::refactor::signature::change(
                    index,
                    the_free_function(index, "width"),
                    fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
                )
                .map(|p| p.edits)
            }),
        ),
        (
            "imports",
            Box::new(|ws: &Workspace, index: &Index| {
                fun_refactor::refactor::imports::plan(index, &ws.path("src/util.rs"))
                    .map(|p| p.edits)
            }),
        ),
    ] {
        let ws = Workspace::new(&awkward());
        let index = ws.index();
        if !gate(what, &ws, plan(&ws, &index)) {
            refused.push(what);
        }
    }
    eprintln!("refused on the awkward crate: {refused:?}");
}

#[test]
fn extracting_an_expression_from_inside_a_closure_compiles_or_refuses() {
    // The binding goes at the start of the enclosing statement, and that statement is outside
    // the closure.
    let mut files = plain();
    files.push((
        "src/counts.rs",
        "use crate::holder::Holder;\n\n\
         pub fn wide(h: &Holder) -> Vec<u8> {\n    \
         h.items.iter().filter(|i| **i > 1 && **i < 9).copied().collect()\n}\n",
    ));
    files[1] = (
        "src/lib.rs",
        "pub mod holder;\npub mod util;\npub mod counts;\n#[cfg(feature = \"extra\")]\npub mod extra;\n",
    );
    let ws = Workspace::new(&files);
    let index = ws.index();
    let source = ws.read("src/counts.rs");
    let at = source.find("**i > 1 && **i < 9").expect("the expression");
    let span = fun_refactor::span::Span::new(at, at + "**i > 1 && **i < 9".len());
    let planned = fun_refactor::refactor::extract::variable(
        &index,
        &ws.path("src/counts.rs"),
        span,
        "wide_enough",
        false,
    )
    .map(|p| p.edits);
    must_refuse(
        "extracting from inside a closure",
        &ws,
        planned,
        "is introduced between the binding's position and this expression",
    );
}

#[test]
fn a_guard_clause_in_a_function_that_returns_a_value_compiles_or_refuses() {
    // An early exit needs a value here, and a bare `return` does not compile.
    let mut files = plain();
    files.push((
        "src/deep.rs",
        "pub fn pick(a: bool, b: bool) -> u8 {\n    match a {\n        true => {\n            \
         {\n                if b {\n                    let _ = 1;\n                }\n            }\n        \
         }\n        false => {}\n    }\n    7\n}\n",
    ));
    files[1] = (
        "src/lib.rs",
        "pub mod holder;\npub mod util;\npub mod deep;\n#[cfg(feature = \"extra\")]\npub mod extra;\n",
    );
    let ws = Workspace::new(&files);
    let index = ws.index();
    let source = ws.read("src/deep.rs");
    let at = source.find("if b {").expect("the if");
    let planned = fun_refactor::refactor::rewrite::apply(
        &index,
        &ws.path("src/deep.rs"),
        at,
        fun_refactor::refactor::rewrite::Rewrite::GuardClause,
    )
    .map(|p| p.edits);
    must_refuse(
        "a guard clause in a function that returns a value",
        &ws,
        planned,
        "this function returns a value, so an early exit needs one too",
    );
}

fn tsc_is_available() -> bool {
    Command::new("tsc")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The same shapes as the Rust fixture, in TypeScript.
fn typescript() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "tsconfig.json",
            // A later TypeScript dropped `moduleResolution: node`, and CI runs one.
            "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\"\n  },\n  \"include\": [\"src\"]\n}\n",
        ),
        (
            "src/holder.ts",
            "export function width(items: number[], n: number): number {\n               return items.length + n;\n}\n\n             export class Holder {\n  items: number[] = [];\n\n               width(n: number): number {\n    return width(this.items, n);\n  }\n}\n",
        ),
        (
            "src/util.ts",
            "import { width, Holder } from \"./holder\";\n\n             export function describe(h: Holder): string {\n               const total = width(h.items, 1);\n  return `${total * 2}`;\n}\n",
        ),
        (
            "src/index.ts",
            "export { width, Holder } from \"./holder\";\nexport { describe } from \"./util\";\n",
        ),
        (
            "src/main.ts",
            "import { Holder, width } from \"./index\";\nimport { describe } from \"./util\";\n\nconst h = new Holder();\nconsole.log(describe(h), h.width(1), width(h.items, 2));\n",
        ),
    ]
}

#[test]
fn the_typescript_fixture_compiles_before_anything_touches_it() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    if let Err(e) = ws.compiles() {
        panic!("the TypeScript fixture is broken before any refactoring:\n{e}");
    }
}

#[test]
fn renaming_a_typescript_function_compiles() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "spanOf").map(|p| p.edits);
    must_plan("renaming a TypeScript function", &ws, planned);
}

#[test]
fn organising_typescript_imports_compiles() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let planned =
        fun_refactor::refactor::imports::plan(&index, &ws.path("src/main.ts")).map(|p| p.edits);
    // Organising an import list that is already tidy may plan nothing.
    match planned {
        Ok(edits) if edits.is_empty() => {}
        other => {
            must_plan("organising TypeScript imports", &ws, other);
        }
    }
}

#[test]
fn moving_a_typescript_symbol_compiles_or_refuses() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("src/util.ts"))
        .map(|p| p.edits);
    must_plan("moving a TypeScript symbol", &ws, planned);
}

#[test]
fn changing_a_typescript_signature_compiles_or_refuses() {
    if !tsc_is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    must_plan("changing a TypeScript signature", &ws, planned);
}

/// A barrel that hands on everything the file next to it exports.
fn typescript_star_barrel() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "tsconfig.json",
            "{\n  \"compilerOptions\": {\n    \"strict\": true,\n    \"noEmit\": true,\n    \"target\": \"ES2020\",\n    \"module\": \"esnext\",\n    \"moduleResolution\": \"bundler\"\n  },\n  \"include\": [\"src\"]\n}\n",
        ),
        (
            "src/holder.ts",
            "export function width(items: number[], n: number): number {\n  return items.length + n;\n}\n\nexport const version = 1;\n",
        ),
        ("src/util.ts", "export const other = 1;\n"),
        ("src/index.ts", "export * from \"./holder\";\n"),
        (
            "src/main.ts",
            "import { width } from \"./index\";\nconsole.log(width([1], 2));\n",
        ),
    ]
}

#[test]
fn moving_out_from_under_a_named_barrel_compiles() {
    if !Toolchain::Tsc.is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("src/util.ts"))
        .map(|p| p.edits);
    must_plan("moving out from under a named barrel", &ws, planned);
    let barrel = ws.read("src/index.ts");
    assert!(
        barrel.contains("width") && barrel.contains("./util"),
        "the barrel still has to hand the name on:\n{barrel}"
    );
    assert!(
        barrel.contains("Holder"),
        "and it must not drop the names beside it:\n{barrel}"
    );
}

#[test]
fn moving_out_from_under_a_star_barrel_compiles() {
    if !Toolchain::Tsc.is_available() {
        eprintln!("compile gate: typescript skipped, tsc is not on PATH");
        return;
    }
    let ws = Workspace::typescript(&typescript_star_barrel());
    if let Err(e) = ws.compiles() {
        panic!("the star barrel fixture is broken before any refactoring:\n{e}");
    }
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("src/util.ts"))
        .map(|p| p.edits);
    must_plan("moving out from under a star barrel", &ws, planned);
    let reader = ws.read("src/main.ts");
    assert_eq!(
        reader.matches("import").count(),
        1,
        "the reader already had the name through the barrel:\n{reader}"
    );
}

/// The same shapes again in Go: a free function and a method sharing a name, an import
/// of the function from another package, and a caller of both.
fn go_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("go.mod", "module gate\n\ngo 1.21\n"),
        (
            "holder/holder.go",
            "package holder\n\n\
             type Holder struct {\n\tItems []byte\n}\n\n\
             func Width(items []byte, n int) int {\n\treturn len(items) + n\n}\n\n\
             func (h *Holder) Width(n int) int {\n\treturn Width(h.Items, n)\n}\n",
        ),
        (
            "util/util.go",
            "package util\n\n\
             import (\n\t\"fmt\"\n\n\t\"gate/holder\"\n)\n\n\
             func Describe(h *holder.Holder) string {\n\t\
             total := holder.Width(h.Items, 1)\n\treturn fmt.Sprintf(\"%d\", total*2)\n}\n",
        ),
        (
            "main.go",
            "package main\n\n\
             import (\n\t\"fmt\"\n\n\t\"gate/holder\"\n\t\"gate/util\"\n)\n\n\
             func main() {\n\t\
             h := &holder.Holder{Items: []byte{1, 2}}\n\t\
             fmt.Println(util.Describe(h), h.Width(1), holder.Width(h.Items, 2))\n}\n",
        ),
    ]
}

#[test]
fn the_go_fixture_compiles_before_anything_touches_it() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    if let Err(e) = ws.compiles() {
        panic!("the Go fixture is broken before any refactoring:\n{e}");
    }
}

#[test]
fn renaming_a_go_function_compiles() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    let index = ws.index();
    let id = the_free_function(&index, "Width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "SpanOf").map(|p| p.edits);
    must_plan("renaming a Go function", &ws, planned);
}

#[test]
fn changing_a_go_signature_compiles_or_refuses() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    let index = ws.index();
    let id = the_free_function(&index, "Width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    must_plan("changing a Go signature", &ws, planned);
}

#[test]
fn moving_a_go_symbol_compiles_or_refuses() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    let index = ws.index();
    let id = the_free_function(&index, "Width");
    let planned =
        fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("util/util.go"))
            .map(|p| p.edits);
    must_refuse(
        "moving a Go symbol",
        &ws,
        planned,
        "is used from 2 file(s) outside package",
    );
}

#[test]
fn organising_go_imports_compiles_or_refuses() {
    if !Toolchain::Go.is_available() {
        eprintln!("compile gate: go skipped, go is not on PATH");
        return;
    }
    let ws = Workspace::go(&go_files());
    let index = ws.index();
    let planned =
        fun_refactor::refactor::imports::plan(&index, &ws.path("main.go")).map(|p| p.edits);
    match planned {
        Ok(edits) if edits.is_empty() => {}
        other => {
            must_plan("organising Go imports", &ws, other);
        }
    }
}

/// The same shapes in Python.
fn python_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "holder.py",
            "def width(items, n):\n    return len(items) + n\n\n\n\
             class Holder:\n    def __init__(self):\n        self.items = []\n\n    \
             def width(self, n):\n        return width(self.items, n)\n",
        ),
        (
            "util.py",
            "from holder import Holder, width\n\n\n\
             def describe(h: Holder) -> str:\n    \
             total = width(h.items, 1)\n    return f\"{total * 2}\"\n",
        ),
        (
            "main.py",
            "from holder import Holder, width\nfrom util import describe\n\n\n\
             def check():\n    h = Holder()\n    h.items = [1, 2]\n    \
             assert describe(h) == \"6\", describe(h)\n    \
             assert h.width(1) == 3, h.width(1)\n    \
             assert width(h.items, 2) == 4, width(h.items, 2)\n",
        ),
    ]
}

#[test]
fn the_python_fixture_runs_before_anything_touches_it() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    if let Err(e) = ws.compiles() {
        panic!("the Python fixture is broken before any refactoring:\n{e}");
    }
}

#[test]
fn renaming_a_python_function_compiles() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "span_of").map(|p| p.edits);
    must_plan("renaming a Python function", &ws, planned);
}

#[test]
fn changing_a_python_signature_compiles_or_refuses() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    must_plan("changing a Python signature", &ws, planned);
}

#[test]
fn moving_a_python_symbol_compiles_or_refuses() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("util.py"))
        .map(|p| p.edits);
    must_refuse(
        "moving a Python symbol",
        &ws,
        planned,
        "would make the two files import each other",
    );
}

#[test]
fn inlining_a_python_variable_compiles_or_refuses() {
    if !Toolchain::Python.is_available() {
        eprintln!("compile gate: python skipped, python3 is not on PATH");
        return;
    }
    let ws = Workspace::python(&python_files());
    let index = ws.index();
    let total = index
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .expect("the local")
        .id;
    let planned = fun_refactor::refactor::inline::variable(&index, total).map(|p| p.edits);
    must_plan("inlining a Python variable", &ws, planned);
}

/// The same shapes again in Zig: a free function and a method of one name, a second file
/// importing the first, and a root that calls both.
fn zig_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "holder.zig",
            "pub const Holder = struct {\n    items: []const u8,\n\n    \
             pub fn width(self: Holder, n: usize) usize {\n        \
             return self.items.len + n;\n    }\n};\n\n\
             pub fn width(items: []const u8, n: usize) usize {\n    return items.len + n;\n}\n",
        ),
        (
            "util.zig",
            "const holder = @import(\"holder.zig\");\n\n\
             pub fn describe(h: holder.Holder) usize {\n    \
             const total = holder.width(h.items, 1);\n    return total * 2;\n}\n",
        ),
        (
            "main.zig",
            "const holder = @import(\"holder.zig\");\nconst util = @import(\"util.zig\");\n\n\
             pub fn main() void {\n    const h = holder.Holder{ .items = \"ab\" };\n    \
             _ = util.describe(h);\n    _ = h.width(1);\n    \
             _ = holder.width(h.items, 2);\n}\n",
        ),
    ]
}

#[test]
fn the_zig_fixture_compiles_before_anything_touches_it() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    if let Err(e) = ws.compiles() {
        panic!("the Zig fixture is broken before any refactoring:\n{e}");
    }
}

#[test]
fn renaming_a_zig_function_compiles() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "spanOf").map(|p| p.edits);
    must_plan("renaming a Zig function", &ws, planned);
}

#[test]
fn changing_a_zig_signature_compiles_or_refuses() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    must_plan("changing a Zig signature", &ws, planned);
}

#[test]
fn moving_a_zig_symbol_compiles_or_refuses() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    let index = ws.index();
    let id = the_free_function(&index, "width");
    let planned = fun_refactor::refactor::move_symbol::to_file(&index, id, &ws.path("util.zig"))
        .map(|p| p.edits);
    must_plan("moving a Zig symbol", &ws, planned);
}

#[test]
fn inlining_a_zig_variable_compiles_or_refuses() {
    if !Toolchain::Zig.is_available() {
        eprintln!("compile gate: zig skipped, zig is not on PATH");
        return;
    }
    let ws = Workspace::zig(&zig_files());
    let index = ws.index();
    let total = index
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .expect("the local")
        .id;
    let planned = fun_refactor::refactor::inline::variable(&index, total).map(|p| p.edits);
    must_plan("inlining a Zig variable", &ws, planned);
}

/// The same shapes in Java, where they land differently: a class has no top level.
fn java_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "Widths.java",
            "public class Widths {\n    public static int width(byte[] items, int n) {\n        \
             return items.length + n;\n    }\n}\n",
        ),
        (
            "Holder.java",
            "public class Holder {\n    public byte[] items = new byte[0];\n\n    \
             public int width(int n) {\n        return Widths.width(items, n);\n    }\n}\n",
        ),
        (
            "Describe.java",
            "public class Describe {\n    public static String describe(Holder h) {\n        \
             int total = Widths.width(h.items, 1);\n        \
             return String.valueOf(total * 2);\n    }\n}\n",
        ),
        (
            "Main.java",
            "public class Main {\n    public static void main(String[] args) {\n        \
             Holder h = new Holder();\n        h.items = new byte[] { 1, 2 };\n        \
             System.out.println(\n            Describe.describe(h) + h.width(1) + \
             Widths.width(h.items, 2));\n    }\n}\n",
        ),
    ]
}

#[test]
fn the_java_fixture_compiles_before_anything_touches_it() {
    if !Toolchain::Javac.is_available() {
        eprintln!("compile gate: java skipped, javac is not on PATH");
        return;
    }
    let ws = Workspace::java(&java_files());
    if let Err(e) = ws.compiles() {
        panic!("the Java fixture is broken before any refactoring:\n{e}");
    }
}

/// The static method, which is the free function's counterpart here.
fn the_static_method(index: &Index, name: &str) -> fun_refactor::model::SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name && s.qualifier.as_deref() == Some("Widths"))
        .unwrap_or_else(|| panic!("no static method named {name} on Widths"))
        .id
}

#[test]
fn renaming_a_java_method_compiles() {
    if !Toolchain::Javac.is_available() {
        eprintln!("compile gate: java skipped, javac is not on PATH");
        return;
    }
    let ws = Workspace::java(&java_files());
    let index = ws.index();
    let id = the_static_method(&index, "width");
    let planned = fun_refactor::refactor::rename::plan(&index, id, "spanOf").map(|p| p.edits);
    must_plan("renaming a Java method", &ws, planned);
}

#[test]
fn changing_a_java_signature_compiles_or_refuses() {
    if !Toolchain::Javac.is_available() {
        eprintln!("compile gate: java skipped, javac is not on PATH");
        return;
    }
    let ws = Workspace::java(&java_files());
    let index = ws.index();
    let id = the_static_method(&index, "width");
    let planned = fun_refactor::refactor::signature::change(
        &index,
        id,
        fun_refactor::refactor::signature::Change::Move { from: 0, to: 1 },
    )
    .map(|p| p.edits);
    must_plan("changing a Java signature", &ws, planned);
}

#[test]
fn inlining_a_java_variable_compiles_or_refuses() {
    if !Toolchain::Javac.is_available() {
        eprintln!("compile gate: java skipped, javac is not on PATH");
        return;
    }
    let ws = Workspace::java(&java_files());
    let index = ws.index();
    let total = index
        .symbols
        .iter()
        .find(|s| s.name == "total")
        .expect("the local")
        .id;
    let planned = fun_refactor::refactor::inline::variable(&index, total).map(|p| p.edits);
    must_plan("inlining a Java variable", &ws, planned);
}

/// The gate has to be able to fail.
#[test]
fn the_gate_reports_a_workspace_that_does_not_compile() {
    let rust = Workspace::new(&plain());
    std::fs::write(
        rust.path("src/util.rs"),
        "pub fn describe() -> String {\n    no_such_function()\n}\n",
    )
    .expect("write");
    assert!(
        rust.compiles().is_err(),
        "cargo check accepted a call to a function that does not exist"
    );

    if !tsc_is_available() {
        eprintln!("compile gate: the TypeScript half of this check was skipped, tsc is absent");
        return;
    }
    let ts = Workspace::typescript(&typescript());
    std::fs::write(
        ts.path("src/util.ts"),
        "import { nothing } from \"./holder\";\nexport const x = nothing;\n",
    )
    .expect("write");
    assert!(
        ts.compiles().is_err(),
        "tsc accepted an import of a name that is not exported"
    );

    if Toolchain::Go.is_available() {
        let go = Workspace::go(&go_files());
        std::fs::write(
            go.path("util/util.go"),
            "package util\n\nimport \"gate/holder\"\n\n\
             func Describe(h *holder.Holder) string {\n\treturn holder.NoSuchFunction(h)\n}\n",
        )
        .expect("write");
        assert!(
            go.compiles().is_err(),
            "go build accepted a call to a function that does not exist"
        );
    } else {
        eprintln!("compile gate: the Go half of this check was skipped, go is absent");
    }

    if Toolchain::Python.is_available() {
        // Python compiles a name that does not exist and fails at the read, which is
        // why this toolchain runs the fixture as well as compiling it.
        let python = Workspace::python(&python_files());
        std::fs::write(
            python.path("util.py"),
            "from holder import Holder, no_such_function\n\n\n\
             def describe(h: Holder) -> str:\n    return no_such_function(h)\n",
        )
        .expect("write");
        assert!(
            python.compiles().is_err(),
            "importing a name that the module does not define was accepted"
        );

        // And the half that only running catches: every name still resolves, and the
        // answer changed.
        let changed = Workspace::python(&python_files());
        std::fs::write(
            changed.path("holder.py"),
            "def width(items, n):\n    return len(items) + n + 1\n\n\n\
             class Holder:\n    def __init__(self):\n        self.items = []\n\n    \
             def width(self, n):\n        return width(self.items, n)\n",
        )
        .expect("write");
        assert!(
            changed.compiles().is_err(),
            "the fixture's assertions did not notice a changed answer"
        );
    } else {
        eprintln!("compile gate: the Python half of this check was skipped, python3 is absent");
    }

    if Toolchain::Zig.is_available() {
        let zig = Workspace::zig(&zig_files());
        std::fs::write(
            zig.path("util.zig"),
            "const holder = @import(\"holder.zig\");\n\n\
             pub fn describe(h: holder.Holder) usize {\n    \
             return holder.noSuchFunction(h);\n}\n",
        )
        .expect("write");
        assert!(
            zig.compiles().is_err(),
            "zig accepted a call to a declaration the imported file does not have"
        );
    } else {
        eprintln!("compile gate: the Zig half of this check was skipped, zig is absent");
    }

    if Toolchain::Javac.is_available() {
        let java = Workspace::java(&java_files());
        std::fs::write(
            java.path("Describe.java"),
            "public class Describe {\n    public static String describe(Holder h) {\n        \
             return Widths.noSuchMethod(h);\n    }\n}\n",
        )
        .expect("write");
        assert!(
            java.compiles().is_err(),
            "javac accepted a call to a method that does not exist"
        );
    } else {
        eprintln!("compile gate: the Java half of this check was skipped, javac is absent");
    }
}

/// What this gate covers, said out loud.
#[test]
fn the_gate_states_what_it_covers() {
    assert!(
        rustc_is_available(),
        "cargo is not on PATH, so every case in this file checked nothing"
    );
    for (language, toolchain) in [
        ("rust", Toolchain::Cargo),
        ("typescript", Toolchain::Tsc),
        ("go", Toolchain::Go),
        ("python", Toolchain::Python),
        ("zig", Toolchain::Zig),
        ("java", Toolchain::Javac),
    ] {
        eprintln!(
            "compile gate: {language} — {} ({})",
            toolchain.covers(),
            match toolchain.is_available() {
                true => "ran here",
                false => "skipped, its toolchain is absent here",
            }
        );
    }
    // The languages this file has no fixture for.
    let driven = ["rust", "typescript", "go", "python", "zig", "java"];
    let rest: Vec<&str> = fun_refactor::lang::Language::ALL
        .iter()
        .map(|l| l.name())
        .filter(|name| !driven.contains(name))
        .collect();
    eprintln!("compile gate: not driven: {}", rest.join(", "));
    assert_eq!(
        driven.len() + rest.len(),
        fun_refactor::lang::Language::ALL.len(),
        "the census names {} language(s) and this tool reads {}",
        driven.len() + rest.len(),
        fun_refactor::lang::Language::ALL.len()
    );
}
