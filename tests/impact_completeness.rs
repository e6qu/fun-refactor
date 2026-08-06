//! What `fr impact` says it looked at.
//!
//! The caller walk is bounded, and the bound is a choice. A five-deep call chain traced
//! three levels reported "affects 4 site(s)" — a definite count of an incomplete search
//! — and said nothing at all about the two functions it had not looked at. This is the
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
