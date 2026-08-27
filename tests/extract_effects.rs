//! Extracting a region that does something the caller can see.

use fun_refactor::index::Index;
use fun_refactor::refactor::extract;
use fun_refactor::scan::ScanOptions;
use fun_refactor::span::{LineCol, LineIndex, Span};

fn region(source: &str, from: (usize, usize), to: (usize, usize)) -> Span {
    let lines = LineIndex::new(source);
    let start = lines
        .offset(
            LineCol {
                line: from.0,
                col: from.1,
            },
            source,
        )
        .expect("the start position");
    let end = lines
        .offset(
            LineCol {
                line: to.0,
                col: to.1,
            },
            source,
        )
        .expect("the end position");
    Span::new(start, end)
}

fn extracted(
    file: &str,
    source: &str,
    from: (usize, usize),
    to: (usize, usize),
    name: &str,
) -> anyhow::Result<String> {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let path = tmp.path().join(file);
    std::fs::write(&path, source).expect("the file");
    let index = Index::build(tmp.path(), &ScanOptions::default()).expect("an index");
    let plan = extract::function(&index, &path, region(source, from, to), name)?;
    Ok(
        fun_refactor::edit::apply_to_string(source, plan.edits.edits_for(&path).expect("edits"))
            .expect("applying"),
    )
}

const TS_ASYNC: &str = "\
async function run(n: number): Promise<number> {
    const first = await fetch1(n);
    const second = first + 1;
    return second;
}
async function fetch1(n: number): Promise<number> { return n; }
";

const PY_ASYNC: &str = "\
async def run(n):
    first = await fetch1(n)
    second = first + 1
    return second

async def fetch1(n):
    return n
";

#[test]
fn an_awaiting_region_becomes_an_async_function() {
    // Without this the body kept an `await` in a function that is not async.
    let after = extracted("c.ts", TS_ASYNC, (2, 5), (3, 31), "fetchIt").expect("an extraction");
    assert!(after.contains("async function fetchIt("), "{after}");
    assert!(after.contains("= await fetchIt(n);"), "{after}");

    let after = extracted("d.py", PY_ASYNC, (2, 5), (3, 23), "fetch_it").expect("an extraction");
    assert!(after.contains("async def fetch_it("), "{after}");
    assert!(after.contains("= await fetch_it(n)"), "{after}");
}

#[test]
fn an_await_this_cannot_move_is_refused() {
    // Rust writes `.await` as a postfix, so there is no keyword to move onto the new function.
    let source = "\
async fn run(n: i32) -> i32 {
    let first: i32 = fetch1(n).await;
    let second: i32 = first + 1;
    second
}

async fn fetch1(n: i32) -> i32 { n }
";
    let error = extracted("a.rs", source, (2, 5), (3, 33), "fetch_it")
        .expect_err("a refusal")
        .to_string();
    assert!(error.contains("awaits"), "{error}");
}

#[test]
fn a_yielding_region_is_refused() {
    // The worst of the three, because it was silent.
    let source = "\
def run(items):
    total = 0
    for item in items:
        yield item
        total += item
    return total
";
    let error = extracted("e.py", source, (4, 9), (5, 23), "step")
        .expect_err("a refusal")
        .to_string();
    assert!(error.contains("yield"), "{error}");

    let source = "\
function* run(items: number[]) {
    let total = 0;
    for (const item of items) {
        yield item;
        total += item;
    }
    return total;
}
";
    let error = extracted("e.ts", source, (4, 9), (5, 22), "step")
        .expect_err("a refusal")
        .to_string();
    assert!(error.contains("yield"), "{error}");
}

#[test]
fn a_region_with_neither_is_untouched_by_any_of_this() {
    let source = "\
function run(n: number): number {
    const base = n * 2;
    const scaled = base + 3;
    return scaled;
}
";
    let after = extracted("f.ts", source, (2, 5), (3, 29), "compute").expect("an extraction");
    assert!(after.contains("function compute("), "{after}");
    assert!(!after.contains("async"), "{after}");
    assert!(!after.contains("await"), "{after}");
}
