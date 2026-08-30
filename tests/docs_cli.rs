//! `CLI.md` documents every command, and only commands that exist.

use std::collections::BTreeSet;
use std::process::Command;

/// Every subcommand `fr --help` lists.
fn commands() -> BTreeSet<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_fr"))
        .arg("--help")
        .output()
        .expect("fr --help runs");
    let text = String::from_utf8_lossy(&out.stdout);
    let mut found = BTreeSet::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with("Commands:") {
            inside = true;
            continue;
        }
        if inside && line.starts_with("Options:") {
            break;
        }
        if !inside {
            continue;
        }
        // A command line is two spaces, the name, then its summary.
        let Some(rest) = line.strip_prefix("  ") else {
            continue;
        };
        let Some(name) = rest.split_whitespace().next() else {
            continue;
        };
        if name.starts_with('-') || name == "help" {
            continue;
        }
        found.insert(name.to_string());
    }
    found
}

/// Every command `CLI.md` gives a heading to.
fn documented() -> BTreeSet<String> {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/CLI.md"))
        .expect("CLI.md is there");
    let mut found = BTreeSet::new();
    for line in text.lines() {
        let Some(heading) = line.strip_prefix("### ") else {
            continue;
        };
        // `### \`fr usages\` and \`fr refs\`` documents two.
        for word in heading.split("` and `") {
            let name = word.trim().trim_matches('`').trim();
            if let Some(command) = name.strip_prefix("fr ") {
                found.insert(command.trim().to_string());
            }
        }
    }
    found
}

#[test]
fn every_command_has_a_section() {
    let missing: Vec<String> = commands().difference(&documented()).cloned().collect();
    assert!(
        missing.is_empty(),
        "CLI.md documents no section for {missing:?}. A command nobody can find \
         in the reference is a command nobody finds."
    );
}

#[test]
fn every_section_is_a_command() {
    let invented: Vec<String> = documented().difference(&commands()).cloned().collect();
    assert!(
        invented.is_empty(),
        "CLI.md has a section for {invented:?}, and `fr` has no such command. \
         A reader following that section reaches a refusal."
    );
}

#[test]
fn the_reference_covers_more_than_a_handful() {
    // A parser that silently found nothing would pass both checks above by
    // comparing one empty set with another.
    let found = commands();
    assert!(
        found.len() > 25,
        "only {} command(s) came out of `fr --help`, so the checks above \
         compared almost nothing: {found:?}",
        found.len()
    );
}
