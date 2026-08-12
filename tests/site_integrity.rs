//! The published site, checked against itself and against the binary.
//!
//! `cargo test --test site_data` already asserts that every result shown on the site is
//! what the tool produced. Nothing asserted the rest of it: that a link goes somewhere,
//! that an anchor names a heading that exists, that a command the prose tells a reader
//! to run is a command. Those are the parts that rot silently, because a dead link
//! looks exactly like a live one until somebody clicks it.
//!
//! Everything here reads the `docs/` tree from disk. No network: a test that needs one
//! fails for reasons that have nothing to do with the change in front of it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn docs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs")
}

fn pages() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![docs()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the docs directory is readable") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "html") {
                let name = path
                    .strip_prefix(docs())
                    .expect("under docs/")
                    .display()
                    .to_string();
                out.push((name, std::fs::read_to_string(&path).expect("a page")));
            }
        }
    }
    out.sort();
    assert!(out.len() >= 8, "found only {} pages", out.len());
    out
}

/// Every `href`/`src` in a page, in source order.
fn references(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for attribute in ["href=\"", "src=\""] {
        let mut rest = html;
        while let Some(at) = rest.find(attribute) {
            rest = &rest[at + attribute.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
                rest = &rest[end..];
            }
        }
    }
    out
}

fn ids(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("id=\"") {
        rest = &rest[at + 4..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// Is this path something the frontend build writes instead of something in the tree?
///
/// The playground is emitted by Vite and is not committed, so on a clean checkout every
/// link to it points at a directory that is not there, while being perfectly live on
/// the published site. Read from the build's own `outDir` and not written down
/// here: a hardcoded exception is a second place to remember, and this test exists
/// because second places to remember are how a site rots.
fn built_by_the_frontend(path: &Path) -> bool {
    let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("web/vite.config.ts");
    let Ok(text) = std::fs::read_to_string(&config) else {
        return false;
    };
    let Some(at) = text.find("outDir:") else {
        return false;
    };
    let rest = &text[at + "outDir:".len()..];
    let Some(open) = rest.find('"') else {
        return false;
    };
    let Some(close) = rest[open + 1..].find('"') else {
        return false;
    };
    let out_dir = &rest[open + 1..open + 1 + close];
    // The path is written relative to `web/`.
    let built = config
        .parent()
        .expect("web/")
        .join(out_dir)
        .components()
        .fold(PathBuf::new(), |mut acc, component| {
            match component {
                std::path::Component::ParentDir => {
                    acc.pop();
                }
                other => acc.push(other),
            }
            acc
        });
    path.starts_with(&built)
}

#[test]
fn every_internal_link_goes_somewhere() {
    let mut broken = Vec::new();
    for (name, html) in pages() {
        for reference in references(&html) {
            if reference.starts_with('#')
                || reference.starts_with("data:")
                || reference.starts_with("mailto:")
                || reference.starts_with("http")
                || reference.is_empty()
            {
                continue;
            }
            let target = reference.split(['#', '?']).next().unwrap_or("");
            if target.is_empty() {
                continue;
            }
            let base = docs().join(&name);
            let resolved = base.parent().expect("a parent").join(target);
            let exists = resolved.exists()
                || resolved.join("index.html").exists()
                || built_by_the_frontend(&resolved);
            if !exists {
                broken.push(format!("{name} -> {reference}"));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "dead link(s):\n  {}",
        broken.join("\n  ")
    );
}

#[test]
fn every_anchor_names_something_on_the_page() {
    // A table of contents pointing at a heading that was renamed scrolls nowhere, and
    // looks exactly like one that works.
    let mut broken = Vec::new();
    for (name, html) in pages() {
        let present: BTreeSet<String> = ids(&html).into_iter().collect();
        for reference in references(&html) {
            if let Some(anchor) = reference.strip_prefix('#') {
                if !anchor.is_empty() && !present.contains(anchor) {
                    broken.push(format!("{name} -> #{anchor}"));
                }
            }
        }
    }
    assert!(
        broken.is_empty(),
        "anchor(s) naming nothing:\n  {}",
        broken.join("\n  ")
    );
}

#[test]
fn no_page_declares_the_same_id_twice() {
    // Two elements with one id means every `#anchor` and `getElementById` picks one of
    // them, and which one is a fact about source order and not about intent.
    let mut duplicated = Vec::new();
    for (name, html) in pages() {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for id in ids(&html) {
            *counts.entry(id).or_default() += 1;
        }
        for (id, count) in counts.into_iter().filter(|(_, c)| *c > 1) {
            duplicated.push(format!("{name}: `{id}` appears {count} times"));
        }
    }
    assert!(
        duplicated.is_empty(),
        "duplicate id(s):\n  {}",
        duplicated.join("\n  ")
    );
}

/// Plain text, with the tags taken out.
fn prose(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut inside = false;
    for character in html.chars() {
        match character {
            '<' => inside = true,
            '>' => {
                inside = false;
                out.push(' ');
            }
            _ if !inside => out.push(character),
            _ => {}
        }
    }
    out.replace("&gt;", ">").replace("&lt;", "<")
}

#[test]
fn every_command_the_site_names_is_a_command() {
    // The site tells a reader what to type. A command that was renamed leaves prose
    // that reads perfectly and does not run.
    let names = fun_refactor::cli::command_names();
    let known: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let mut unknown = BTreeSet::new();
    for (name, html) in pages() {
        let text = prose(&html);
        for (at, _) in text.match_indices("fr ") {
            let rest = &text[at + 3..];
            let word: String = rest
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || *c == '-')
                .collect();
            // `fr` also appears mid-sentence in prose; only a word that looks like a
            // subcommand is a claim about one.
            if word.len() > 2 && !known.contains(word.as_str()) {
                unknown.insert(format!("{name}: `fr {word}`"));
            }
        }
    }
    assert!(
        unknown.is_empty(),
        "the site names command(s) the binary does not have:\n  {}",
        unknown.iter().cloned().collect::<Vec<_>>().join("\n  ")
    );
}

#[test]
fn every_page_says_what_it_was_built_from() {
    // A failed deploy leaves the site silently stale: green tests, a finished-looking
    // page, and several commits of drift with nothing saying so. Five deploys in a row
    // aborted that way before anybody looked, and the only reason it came to light was
    // somebody asking. The stamp is useful only if it is on every page, because the one
    // a reader happens to open is the one that has to tell them.
    let mut missing = Vec::new();
    for (name, html) in pages() {
        // The playground's page is emitted by Vite and is not ours to edit.
        if built_by_the_frontend(&docs().join(&name)) {
            continue;
        }
        if !html.contains("id=\"built\"") {
            missing.push(format!("{name}: no build stamp in the footer"));
        }
        if !html.contains("built.js") {
            missing.push(format!("{name}: does not load built.js"));
        }
    }
    assert!(
        missing.is_empty(),
        "page(s) that cannot say what they were built from:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn a_parse_failure_says_where_it_is() {
    // "2 error node(s)" and a filename is a report nobody can act on: the whole value of
    // knowing a file did not parse is being able to go and look at the part that did
    // not. Found on vuejs/core, where four files failed and the report gave no position
    // for any of them.
    let tmp = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        tmp.path().join("a.rs"),
        "fn ok() {}\n\nfn broken( { let x = ; }\n",
    )
    .expect("the file");

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["parse", "-C"])
        .arg(tmp.path())
        .output()
        .expect("running fr");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("error node(s)"), "{text}");
    assert!(
        text.contains("a.rs:3:"),
        "the failure names no line: {text}"
    );
}

#[test]
fn the_plan_s_closing_list_names_every_command() {
    // The other direction from the test above: not "does this named command exist" but
    // "is any command missing from the list that claims to enumerate them". `usages`,
    // `implementations`, `recipe` and `translate` had all been shipped and none of them
    // reached the list, which is the failure mode a summary line invites. It is never
    // wrong about what it says, only about what it leaves out.
    let plan =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("PLAN.md"))
            .expect("PLAN.md");
    let listed: BTreeSet<String> = plan
        .rsplit_once("\nCommands:")
        .expect("PLAN.md ends with the list of commands")
        .1
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect();

    let missing: Vec<String> = fun_refactor::cli::command_names()
        .into_iter()
        .filter(|name| !listed.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "PLAN.md's closing list does not name: {}",
        missing.join(", ")
    );
}

#[test]
fn every_published_page_parses() {
    // The site is HTML this tool claims to read, and it shipped two raw `&&` in text,
    // an unterminated entity reference, which browsers recover from and the tool's own
    // parser reports. Nothing checked: the tests here follow links and check command
    // names, which both pass on a file that does not parse.
    let parsers = fun_refactor::parse::Parsers::new();
    let mut broken = Vec::new();
    for (name, html) in pages() {
        let parsed = parsers
            .parse(fun_refactor::lang::Language::Html, &html)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        if parsed.has_errors() {
            broken.push(name);
        }
    }
    assert!(
        broken.is_empty(),
        "page(s) the tool cannot parse: {}",
        broken.join(", ")
    );
}
