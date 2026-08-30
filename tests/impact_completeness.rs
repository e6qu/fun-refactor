//! What `fr impact` says it looked at.

use fun_refactor::analysis::impact;
use fun_refactor::index::Index;
use fun_refactor::scan::ScanOptions;

const CHAIN: &str = "\
def l0(x):
    return x + 1

def l1(x):
    return l0(x)

def l2(x):
    return l1(x)

def l3(x):
    return l2(x)

def l4(x):
    return l3(x)

def l5(x):
    return l4(x)
";

fn analysed(source: &str, symbol: &str, depth: usize) -> impact::Impact {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(tmp.path().join("a.py"), source).expect("the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let id = index
        .symbols
        .iter()
        .find(|s| s.name == symbol)
        .unwrap_or_else(|| panic!("no `{symbol}`"))
        .id;
    impact::analyse(&index, id, depth).expect("an impact")
}

#[test]
fn a_walk_that_stopped_short_says_so() {
    let cut = analysed(CHAIN, "l0", 3);
    assert!(
        cut.callers_beyond_the_depth_limit > 0,
        "a three-deep walk of a five-deep chain reported nothing left over"
    );
    let report = {
        let tmp = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(tmp.path().join("a.py"), CHAIN).expect("the file");
        let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
        impact::format_report(&index, &cut)
    };
    assert!(report.contains("not everything there is"), "{report}");
}

#[test]
fn a_walk_that_finished_says_nothing() {
    // A note nobody needs is how a note somebody needs gets missed.
    let whole = analysed(CHAIN, "l0", 10);
    assert_eq!(whole.callers_beyond_the_depth_limit, 0);
}

#[test]
fn raising_the_depth_finds_what_the_note_promised() {
    let cut = analysed(CHAIN, "l0", 3);
    let whole = analysed(CHAIN, "l0", 10);
    assert!(
        whole.items.len() > cut.items.len(),
        "the note said there was more and there was not: {} vs {}.",
        whole.items.len(),
        cut.items.len()
    );
}

#[test]
fn impact_accounts_for_every_site_a_rename_would_show() {
    // The tool suggests `fr impact` before a change, and `fr rename` is the change.
    use fun_refactor::refactor::rename;
    use fun_refactor::span::LineIndex;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    let tmp = tempfile::tempdir().expect("a temporary directory");
    for (name, text) in [
        (
            "shop/pricing.py",
            "def apply_discount(p, r):\n    return p\n",
        ),
        (
            "shop/__init__.py",
            "from shop.pricing import apply_discount\n\n__all__ = [\"apply_discount\"]\n",
        ),
        ("README.md", "Call `apply_discount` with a rate.\n"),
    ] {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).expect("the directory");
        std::fs::write(path, text).expect("the file");
    }
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let symbol = index
        .symbols
        .iter()
        .find(|s| s.name == "apply_discount")
        .expect("the declaration");

    let plan = rename::plan(&index, symbol.id, "reduce").expect("a rename plan");
    let mut shown: BTreeSet<(PathBuf, usize, usize)> = BTreeSet::new();
    for (path, edits) in plan.edits.iter() {
        let source = std::fs::read_to_string(path).expect("the file");
        let lines = LineIndex::new(&source);
        for edit in edits {
            // The declaration is the thing that changes, not part of what it affects.
            if path == &symbol.file && edit.span == symbol.name_span {
                continue;
            }
            let at = lines.line_col(edit.span.start, &source);
            shown.insert((path.clone(), at.line, at.col));
        }
    }
    for warning in &plan.warnings {
        shown.insert((warning.file.clone(), warning.line, warning.col));
    }

    let impact = impact::analyse(&index, symbol.id, 5).expect("an impact");
    let covered: BTreeSet<(PathBuf, usize, usize)> = impact
        .items
        .iter()
        .map(|item| (item.file.clone(), item.line, item.col))
        .collect();

    assert!(
        shown.len() >= 3,
        "the fixture should show three sites: {shown:?}."
    );
    let missing: Vec<_> = shown.difference(&covered).collect();
    assert!(
        missing.is_empty(),
        "the rename shows sites the impact never mentions: {missing:?}."
    );
}

#[test]
fn duplicates_names_the_threshold_it_searched_with() {
    // The empty answer already said "No duplication of 60 tokens or more".
    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        tmp.path().join("a.py"),
        "def first(order):\n    total = 0\n    for line in order.lines:\n        \
         total = total + line.price\n    return total\n\n\
         def second(basket):\n    amount = 0\n    for row in basket.lines:\n        \
         amount = amount + row.price\n    return amount\n",
    )
    .expect("the file");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["duplicates", "--min-tokens", "20", "-C"])
        .arg(tmp.path())
        .output()
        .expect("running fr");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("20 tokens or more"),
        "the threshold is not in the answer:\n{text}"
    );
    assert!(
        text.contains("fall below the line --min-tokens draws"),
        "nothing says the count leaves smaller copies out:\n{text}"
    );
}
