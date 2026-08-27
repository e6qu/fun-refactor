//! Every file, asked to be written as every language.

use fun_refactor::lang::Language;
use fun_refactor::{translate, transpile};
use std::path::{Path, PathBuf};

/// One small file per language, so every pair gets asked about.
fn corpus() -> Vec<(&'static str, &'static str)> {
    vec![
        ("a.rs", "pub fn width(items: &[u8], n: usize) -> usize {\n    items.len() + n\n}\n"),
        ("b.go", "package b\n\nfunc Width(items []byte, n int) int {\n\treturn len(items) + n\n}\n"),
        ("c.zig", "pub fn width(items: []const u8, n: usize) usize {\n    return items.len + n;\n}\n"),
        ("D.java", "class D {\n  static int width(byte[] items, int n) {\n    return items.length + n;\n  }\n}\n"),
        ("e.ts", "export function width(items: number[], n: number): number {\n  return items.length + n;\n}\n"),
        ("f.tsx", "export function width(items: number[], n: number): number {\n  return items.length + n;\n}\n"),
        ("g.py", "def width(items, n):\n    return len(items) + n\n"),
        ("h.sh", "width() {\n  echo $1\n}\n"),
        ("i.html", "<html><body><p>text</p></body></html>\n"),
        ("j.css", ".thing {\n  color: red;\n}\n"),
        ("k.scss", ".thing {\n  color: red;\n}\n"),
        ("l.tf", "variable \"enabled\" {\n  type = bool\n}\n"),
        ("m.yaml", "name: thing\nvalue: 1\n"),
        ("templates/n.yaml", "name: thing\nvalue: 1\n"),
        ("o.xml", "<root><child>text</child></root>\n"),
        ("p.md", "# Title\n\nSome text.\n"),
    ]
}

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    for (name, content) in corpus() {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(&path, content).expect("write");
    }
    // A `templates/` directory beside a chart file makes a YAML file Helm.
    std::fs::write(
        dir.path().join("Chart.yaml"),
        "name: chart\nversion: 0.1.0\n",
    )
    .expect("write");
    std::fs::write(dir.path().join("values.yaml"), "name: thing\n").expect("write");
    dir
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            match path.is_dir() {
                true => stack.push(path),
                false => out.push(path),
            }
        }
    }
    out.sort();
    out
}

/// Ask for one target the way the command does.
fn ask_for(path: &Path, target: Language) -> Result<PathBuf, String> {
    let from = fun_refactor::lang::detect(path).ok_or("no language")?;
    match translate::targets(from).contains(&target) {
        true => translate::plan(path, target)
            .map(|p| p.destination)
            .map_err(|e| e.to_string()),
        false => transpile::plan(path, target)
            .map(|p| p.destination)
            .map_err(|e| e.to_string()),
    }
}

#[test]
fn everything_the_listing_offers_can_be_asked_for() {
    // A blocked entry is the other half of the promise.
    let tmp = workspace();
    let mut asked = 0;
    let mut blocked = 0;
    for path in files(tmp.path()) {
        for option in translate::options_for(&path) {
            if let Some(reason) = &option.blocked {
                blocked += 1;
                let refused = ask_for(&path, option.target).expect_err("blocked, then produced.");
                assert!(
                    refused.contains("already exists"),
                    "{} is blocked for '{reason}' and refused for: {refused}.",
                    path.display()
                );
                continue;
            }
            asked += 1;
            let got = ask_for(&path, option.target).unwrap_or_else(|e| {
                panic!(
                    "{} lists {} and then declines it: {e}",
                    path.display(),
                    option.target
                )
            });
            assert_eq!(
                got,
                option.destination,
                "{} lists {} going to one place and writes it to another",
                path.display(),
                option.target
            );
        }
    }
    assert!(
        asked >= 30,
        "only {asked} options were offered across the whole corpus, so this checked \
         almost nothing"
    );
    eprintln!(
        "translate sweep: {asked} open options honoured, {blocked} blocked ones \
         refused with their reason"
    );
}

#[test]
fn nothing_that_works_is_left_off_the_listing() {
    let tmp = workspace();
    let mut unlisted = Vec::new();
    for path in files(tmp.path()) {
        let listed: Vec<Language> = translate::options_for(&path)
            .into_iter()
            .map(|o| o.target)
            .collect();
        for target in Language::ALL {
            if listed.contains(target) {
                continue;
            }
            if ask_for(&path, *target).is_ok() {
                unlisted.push(format!("{} -> {target}", path.display()));
            }
        }
    }
    assert!(
        unlisted.is_empty(),
        "{} targets work and are never offered: {unlisted:?}",
        unlisted.len()
    );
}

#[test]
fn a_next_js_route_is_offered_as_a_fastapi_router() {
    // The command's help documents `fastapi` as a target.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let route = tmp.path().join("app/api/posts/route.ts");
    std::fs::create_dir_all(route.parent().expect("a parent")).expect("mkdir");
    std::fs::write(
        &route,
        "export async function GET(req: Request): Promise<Response> {\n  \
         return new Response(\"[]\");\n}\n",
    )
    .expect("write");

    assert!(
        transpile::nextjs::is_api_route(&route),
        "the fixture has to be a route for this to mean anything"
    );
    let planned = transpile::nextjs::plan(&route).expect("a plan for the route");
    assert_eq!(planned.route, "/posts");
    assert!(planned.methods.contains(&"GET".to_string()));
}

#[test]
fn tsx_is_a_target_only_where_the_source_is_already_typescript() {
    // TSX is TypeScript with JSX in it.
    let tmp = workspace();

    let from_typescript = ask_for(&tmp.path().join("e.ts"), Language::Tsx);
    assert!(
        from_typescript.is_ok(),
        "a TypeScript file is TSX: {from_typescript:?}"
    );

    let from_python = ask_for(&tmp.path().join("g.py"), Language::Tsx)
        .expect_err("a translation writes no JSX, so TSX is not a target");
    assert!(from_python.contains("typescript"), "{from_python}");
}

#[test]
fn a_language_with_nowhere_to_go_says_so() {
    let tmp = workspace();
    let sass = tmp.path().join("h.sass");
    std::fs::write(&sass, ".panel\n  color: red\n").expect("the file");
    assert!(
        translate::options_for(&sass).is_empty(),
        "indented sass has no containing grammar and no reader"
    );
    let why = translate::why_nothing(Language::Sass);
    assert!(!why.is_empty(), "the reason has to be sayable");
}
