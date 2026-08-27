//! The examples the site renders carry no comments.
//!
//! Every page under `docs/` shows source this repository wrote, beside prose
//! explaining it. A comment inside the example says the same thing a second
//! time, in the place a reader is looking for the code.
//!
//! `tests/typesafety.rs` has enforced this for its own examples since the rule
//! was decided. The fixtures behind the catalogue, the recipe tutorial and the
//! translation page were written before it and kept theirs: a `"""The distance
//! around a circle."""` over a function named `circ`, a `// Reading is one
//! sample from a sensor.` over `type Reading struct`.
//!
//! Two kinds of comment do reach those pages and belong there. What `fr` itself
//! writes into a translation is the tool speaking, and the pages are about that
//! output. What a vendored corpus file carries is somebody else's code, held to
//! a checksum. Neither is authored here, so neither is what this reads.

use std::collections::BTreeMap;

const SOURCE: &str = include_str!("site_data.rs");

/// Every `const NAME: &str = r#"…"#;` fixture in the generator, by name.
///
/// The raw-string form only. Every fixture in that file uses it, because the
/// contents are source code full of quotes and backslashes.
fn fixtures() -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let mut rest = SOURCE;
    while let Some(at) = rest.find("\nconst ") {
        let after = &rest[at + "\nconst ".len()..];
        let Some(colon) = after.find(':') else { break };
        let name = after[..colon].trim().to_string();
        rest = after;
        let Some(open) = after.find("r#\"") else {
            continue;
        };
        // A `const` whose body starts far past its declaration is a different
        // item; the fixture's own string opens on the same line.
        if after[..open].contains('\n') {
            continue;
        }
        let body = &after[open + 3..];
        let Some(close) = body.find("\"#") else {
            continue;
        };
        found.insert(name, body[..close].to_string());
    }
    found
}

/// Is this line a comment a person wrote, in any language these fixtures use?
///
/// The markers, and what each would otherwise hit. `#` needs the space after
/// it, because a CSS `#panel {` is a selector and a Rust `#[derive]` is an
/// attribute. `#!` is a shebang, which a shell fixture needs to run.
fn is_comment(line: &str) -> bool {
    let text = line.trim_start();
    if text.starts_with("#!") {
        return false;
    }
    text.starts_with("//")
        || text.starts_with("/*")
        || text.starts_with("<!--")
        || text.starts_with("# ")
        || text.starts_with("\"\"\"")
        || text.contains("  // ")
        || text.contains("  # ")
}

#[test]
fn no_rendered_example_explains_itself_in_a_comment() {
    let mut offenders = Vec::new();
    for (name, body) in fixtures() {
        for (at, line) in body.lines().enumerate() {
            if is_comment(line) {
                offenders.push(format!("{name} line {}: {}", at + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these rendered examples explain themselves in comments rather than in \
         the prose beside them: {offenders:#?}"
    );
}

#[test]
fn the_check_found_the_examples() {
    // A scanner that matched nothing would pass the check above in silence.
    let found = fixtures();
    assert!(
        found.len() > 20,
        "only {} fixture(s) were read out of the generator: {:?}.",
        found.len(),
        found.keys().collect::<Vec<_>>()
    );
    // And it has to be reading the bodies, not just the names.
    let total: usize = found.values().map(|b| b.lines().count()).sum();
    assert!(
        total > 200,
        "the fixtures read as {total} line(s) in total, so their bodies did not \
         come through."
    );
}

/// Every `<pre>` block on a page under `docs/`, with the page it came from.
///
/// Tag-stripped and entity-decoded, since the pages mark keywords up inside the
/// block. A comment wrapped in `<em>` is still a comment a reader sees.
fn rendered_blocks() -> Vec<(String, String)> {
    let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    let mut out = Vec::new();
    let mut pages: Vec<std::path::PathBuf> = std::fs::read_dir(&docs)
        .expect("docs/ is readable")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("html"))
        .collect();
    pages.sort();
    for page in pages {
        let text = std::fs::read_to_string(&page).expect("the page is readable");
        let name = page
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut rest = text.as_str();
        while let Some(open) = rest.find("<pre") {
            let after = &rest[open..];
            let Some(start) = after.find('>') else { break };
            let Some(end) = after.find("</pre>") else {
                break;
            };
            out.push((name.clone(), strip(&after[start + 1..end])));
            rest = &after[end + "</pre>".len()..];
        }
    }
    out
}

/// Tags removed and the five HTML entities these pages use decoded.
fn strip(block: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in block.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            other if !inside => out.push(other),
            _ => {}
        }
    }
    out.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[test]
fn no_code_block_on_a_page_carries_a_comment() {
    // The generated blocks come from the fixtures above and from what `fr`
    // itself wrote, so this reads the blocks the pages hold in their own source:
    // the shape of a recipe, a handler beside its translation, the first three
    // commands to run. Each explained itself in a comment while the paragraph
    // under it explained the same thing again.
    let mut offenders = Vec::new();
    for (page, block) in rendered_blocks() {
        for (at, line) in block.lines().enumerate() {
            if is_comment(line) {
                offenders.push(format!("{page} block line {}: {}", at + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these code blocks explain themselves in comments rather than in the \
         prose beside them: {offenders:#?}."
    );
}

#[test]
fn the_check_found_the_blocks() {
    // Most panes on these pages are built by script from the generated data,
    // so the literal blocks are few. They are the ones nothing else checks.
    let blocks = rendered_blocks();
    assert!(
        blocks.len() >= 12,
        "only {} code block(s) were read out of the pages, and the pages hold \
         more than that.",
        blocks.len()
    );
    let lines: usize = blocks.iter().map(|(_, b)| b.lines().count()).sum();
    assert!(
        lines > 40,
        "the blocks read as {lines} line(s), so their contents did not come \
         through the tag stripping."
    );
}
