//! What `fr impact` says it looked at.
//!
//! The caller walk is bounded, and the bound is a choice. A five-deep call chain traced
//! three levels reported "affects 4 site(s)", a definite count of an incomplete search,
//! and said nothing at all about the two functions it had not looked at. This is the
//! command a person uses to decide whether a change is safe.

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
        "the note said there was more and there was not: {} vs {}",
        whole.items.len(),
        cut.items.len()
    );
}

// ------------------------------------------------- the same question of duplicates

#[test]
fn duplicates_names_the_threshold_it_searched_with() {
    // The empty answer already said "No duplication of 60 tokens or more". The
    // non-empty one said "3 duplicated block(s)" and stopped, which reads as all of
    // them, and the non-empty one is the answer somebody acts on.
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
        text.contains("--min-tokens decides where the"),
        "nothing says smaller copies were not counted:\n{text}"
    );
}
