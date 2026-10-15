//! Every document is reachable, and every link between them resolves.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every Markdown file this repository publishes at its top level, and in `docs/`.
fn documents() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for dir in ["", "docs"] {
        let Ok(entries) = std::fs::read_dir(root().join(dir)) else {
            continue;
        };
        for entry in entries {
            let path = entry.expect("an entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let name = path
                .strip_prefix(root())
                .expect("under the root")
                .to_string_lossy()
                .to_string();
            found.insert(name);
        }
    }
    found
}

/// The relative Markdown link targets in one file.
fn links_in(text: &str) -> Vec<String> {
    let text = without_code(text);
    let mut found = Vec::new();
    let mut rest = text.as_str();
    while let Some(open) = rest.find("](") {
        let after = &rest[open + 2..];
        let Some(close) = after.find(')') else { break };
        let target = &after[..close];
        rest = &after[close..];
        if target.starts_with("http") || target.starts_with('#') || target.is_empty() {
            continue;
        }
        found.push(target.split('#').next().unwrap_or(target).to_string());
    }
    found
}

fn without_code(text: &str) -> String {
    let mut prose = String::new();
    let mut fence = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        let marker = trimmed.as_bytes().first().copied();
        let width = trimmed
            .bytes()
            .take_while(|byte| Some(*byte) == marker)
            .count();
        if matches!(marker, Some(b'`' | b'~')) && width >= 3 {
            if let Some((opening, length)) = fence {
                if marker == Some(opening) && width >= length && trimmed[width..].trim().is_empty()
                {
                    fence = None;
                }
                prose.push('\n');
                continue;
            }
            if marker != Some(b'`') || !trimmed[width..].contains('`') {
                fence = Some((marker.unwrap(), width));
                prose.push('\n');
                continue;
            }
        }
        if fence.is_none() {
            prose.push_str(line);
        }
        prose.push('\n');
    }
    let mut visible = String::new();
    let mut rest = prose.as_str();
    while let Some(start) = rest.find('`') {
        if rest[..start]
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'\\')
            .count()
            % 2
            == 1
        {
            visible.push_str(&rest[..start + 1]);
            rest = &rest[start + 1..];
            continue;
        }
        visible.push_str(&rest[..start]);
        rest = &rest[start..];
        let width = rest.bytes().take_while(|byte| *byte == b'`').count();
        let mut search = width;
        let mut closing = None;
        while let Some(offset) = rest[search..].find('`') {
            let position = search + offset;
            if rest[width..position].contains("\n\n") {
                break;
            }
            let length = rest[position..]
                .bytes()
                .take_while(|byte| *byte == b'`')
                .count();
            if length == width {
                closing = Some(position + length);
                break;
            }
            search = position + length;
        }
        if let Some(end) = closing {
            visible.push(' ');
            rest = &rest[end..];
        } else {
            visible.push_str(&rest[..width]);
            rest = &rest[width..];
        }
    }
    visible.push_str(rest);
    visible
}

#[test]
fn link_extraction_ignores_code_and_retains_real_targets() {
    assert_eq!(
        links_in("`handlers['send'](read_input())` and [missing](missing.md)"),
        ["missing.md"]
    );
    assert_eq!(
        links_in("``code `items[0](value)` here`` [real](guide.md#example)"),
        ["guide.md"]
    );
    assert_eq!(
        links_in("[`helper['send'](value)`](guide.md)"),
        ["guide.md"]
    );
    assert_eq!(links_in("`unclosed [real](missing.md)"), ["missing.md"]);
    assert_eq!(
        links_in("\\`literal [real](missing.md) \\`"),
        ["missing.md"]
    );
    assert_eq!(
        links_in("`` unmatched ` [real](missing.md)"),
        ["missing.md"]
    );
}

#[test]
fn link_extraction_ignores_fenced_code_without_hiding_following_links() {
    for (opening, closing, inner) in [
        ("```python", "```", "``"),
        ("~~~~text", "~~~~~", "```"),
        ("````", "`````", "```"),
    ] {
        let text = format!(
            "[before](before.md)\n{opening}\n[example](fake.md)\n{inner}\n{closing}\n[after](after.md)"
        );
        assert_eq!(links_in(&text), ["before.md", "after.md"]);
    }
    assert_eq!(
        links_in("```inline [fake](fake.md)``` [real](guide.md)"),
        ["guide.md"]
    );
}

fn repository_name(from: &str, target: &str) -> Option<String> {
    let base = root().join(from).parent()?.to_path_buf();
    let path = base.join(target).canonicalize().ok()?;
    Some(
        path.strip_prefix(root())
            .ok()?
            .to_string_lossy()
            .to_string(),
    )
}

#[test]
fn every_document_is_reachable_from_the_readme() {
    let published = documents();
    let mut reachable = BTreeSet::from(["README.md".to_string()]);
    let mut pending = VecDeque::from(["README.md".to_string()]);
    while let Some(name) = pending.pop_front() {
        let text = std::fs::read_to_string(root().join(&name)).expect("the document is readable");
        for target in links_in(&text) {
            let Some(linked) = repository_name(&name, &target) else {
                continue;
            };
            if published.contains(&linked) && reachable.insert(linked.clone()) {
                pending.push_back(linked);
            }
        }
    }
    let orphaned: Vec<String> = published
        .into_iter()
        .filter(|name| !reachable.contains(name))
        .collect();
    assert!(
        orphaned.is_empty(),
        "no link path from README.md reaches {orphaned:?}. Add them to the \
         documentation map or another reachable guide."
    );
}

#[test]
fn every_link_between_documents_resolves() {
    let mut dead = Vec::new();
    for name in documents() {
        let path = root().join(&name);
        let text = std::fs::read_to_string(&path).expect("the document is readable");
        let base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        for target in links_in(&text) {
            if !base.join(&target).exists() {
                dead.push(format!("{name} -> {target}"));
            }
        }
    }
    assert!(
        dead.is_empty(),
        "these links point at nothing: {dead:?}. Markdown reports no error for \
         one, so a reader finds it instead."
    );
}

#[test]
fn the_check_found_the_documents_to_check() {
    // Both checks above pass over an empty set without complaining.
    let found = documents();
    assert!(
        found.len() > 8,
        "only {} document(s) turned up, so the checks above compared almost \
         nothing: {found:?}",
        found.len()
    );
}
