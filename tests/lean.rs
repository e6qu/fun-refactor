//! Lean 4. A `def` and a `theorem` are one node told apart by a field. A rename reaches
//! into a proof, because a tactic names the lemmas it rewrites with.

use fun_refactor::index::Index;
use fun_refactor::lang::Language;
use fun_refactor::model::SymbolKind;
use fun_refactor::parse::Parsers;
use fun_refactor::refactor::{move_symbol, rename};
use fun_refactor::scan::{scan, ScanOptions};
use std::path::PathBuf;

mod common;
use common::workspace;

const GEOMETRY: &str = "\
namespace Geometry

structure Point where
  x : Int
  y : Int
  deriving Repr

def translate (p : Point) (dx : Int) : Point :=
  { p with x := p.x + dx }

theorem translate_zero (p : Point) : translate p 0 = p := by
  simp [translate]

inductive Shape where
  | circle (r : Int) : Shape
  | rect (w h : Int) : Shape

def area : Shape → Int
  | .circle r => 3 * r * r
  | .rect w h => w * h

end Geometry
";

fn symbol(index: &Index, name: &str) -> fun_refactor::model::SymbolId {
    index
        .symbols
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no symbol named {name}"))
        .id
}

fn applied(root: &std::path::Path, file: &str, edits: &fun_refactor::edit::EditSet) -> String {
    let path = root.join(file);
    let before = std::fs::read_to_string(&path).unwrap();
    match edits.edits_for(&path) {
        Some(list) => fun_refactor::edit::apply_to_string(&before, list).unwrap(),
        None => before,
    }
}

#[test]
fn ordinary_lean_parses_with_no_errors() {
    let parsed = Parsers::new().parse(Language::Lean, GEOMETRY).unwrap();
    assert!(
        !parsed.has_errors(),
        "ordinary Lean must parse: {:?}",
        parsed.error_spans()
    );
}

#[test]
fn the_declarations_a_file_makes_are_symbols() {
    let (_tmp, index) = workspace(&[("Geometry.lean", GEOMETRY)]);
    let found: Vec<(String, SymbolKind)> = index
        .symbols
        .iter()
        .map(|s| (s.name.clone(), s.kind))
        .collect();

    for wanted in [
        ("Point", SymbolKind::Struct),
        ("x", SymbolKind::Field),
        ("translate", SymbolKind::Function),
        ("translate_zero", SymbolKind::Function),
        ("Shape", SymbolKind::Enum),
        ("circle", SymbolKind::Field),
        ("area", SymbolKind::Function),
    ] {
        assert!(
            found.contains(&(wanted.0.to_string(), wanted.1)),
            "{wanted:?} is missing from {found:?}"
        );
    }
}

#[test]
fn a_rename_reaches_the_uses_inside_a_proof() {
    // `simp [translate]` names the lemma the tactic rewrites with.
    let (tmp, index) = workspace(&[("Geometry.lean", GEOMETRY)]);
    let plan = rename::plan(&index, symbol(&index, "translate"), "shift").unwrap();
    let out = applied(tmp.path(), "Geometry.lean", &plan.edits);

    assert!(out.contains("def shift (p : Point)"), "got:\n{out}");
    assert!(
        out.contains("theorem translate_zero (p : Point) : shift p 0 = p"),
        "the statement names it too:\n{out}"
    );
    assert!(out.contains("simp [shift]"), "the tactic names it:\n{out}");
    assert!(!out.contains("translate p 0"), "got:\n{out}");
}

#[test]
fn a_rename_leaves_a_name_that_only_looks_alike() {
    let (tmp, index) = workspace(&[("Geometry.lean", GEOMETRY)]);
    let plan = rename::plan(&index, symbol(&index, "translate"), "shift").unwrap();
    let out = applied(tmp.path(), "Geometry.lean", &plan.edits);
    assert!(
        out.contains("theorem translate_zero"),
        "the longer name is not this one:\n{out}"
    );
}

#[test]
fn moving_a_declaration_writes_the_import_its_users_need() {
    let tmp = tempfile::tempdir().unwrap();
    for (name, body) in [
        ("lean-toolchain", "leanprover/lean4:v4.15.0\n"),
        ("Geometry.lean", "def area (r : Int) : Int := 3 * r * r\n"),
        (
            "Main.lean",
            "import Geometry\n\ndef main : IO Unit :=\n  IO.println (toString (area 2))\n",
        ),
    ] {
        std::fs::write(tmp.path().join(name), body).unwrap();
    }
    let scanned = scan(tmp.path(), &ScanOptions::default()).unwrap();
    let index = Index::build_from_scan(&scanned).unwrap();

    let plan = move_symbol::to_file(
        &index,
        symbol(&index, "area"),
        &tmp.path().join("Shapes.lean"),
    )
    .expect("a top-level declaration moves between Lean modules");

    let main = applied(tmp.path(), "Main.lean", &plan.edits);
    assert!(
        main.starts_with("import Geometry\nimport Shapes\n"),
        "the import goes beside the ones already there. Lean takes them all before \
         any other command:\n{main}"
    );
    let shapes: PathBuf = tmp.path().join("Shapes.lean");
    let landed = plan
        .edits
        .edits_for(&shapes)
        .expect("the destination gains the declaration");
    assert!(
        landed.iter().any(|e| e.replacement.contains("def area")),
        "the declaration lands: {landed:?}"
    );
}

#[test]
fn the_capability_matrix_says_what_lean_does_and_does_not() {
    use fun_refactor::capabilities::{support, Capability, Support};
    for capability in [
        Capability::Symbols,
        Capability::Rename,
        Capability::MoveToFile,
    ] {
        assert!(
            support(capability, Language::Lean).is_yes(),
            "{capability:?} works for Lean"
        );
    }
    // A reason has to be about Lean and not about another language.
    for capability in Capability::ALL {
        if let Support::NotApplicable { because } = support(*capability, Language::Lean) {
            assert!(
                !because.contains("a method here needs"),
                "{capability:?} gives Lean a reason written for Java: {because}"
            );
        }
    }
}
