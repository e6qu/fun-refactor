use super::*;

fn headers() -> String {
    format!("# branch.oid {}\0# branch.head main\0", "a".repeat(40))
}

fn ordinary(xy: &str, path: &str) -> String {
    format!(
        "1 {xy} N... 100644 100644 100644 {} {} {path}\0",
        "a".repeat(40),
        "b".repeat(40)
    )
}

#[test]
fn parses_all_record_classes_and_preserves_paths_and_conflict_meanings() {
    let mut raw = headers();
    raw.push_str(&ordinary("MM", "mixed file\n.rs"));
    raw.push_str(&format!(
        "2 R. N... 100644 100644 100644 {} {} R100 new name\0old\tname\0",
        "a".repeat(40),
        "b".repeat(40)
    ));
    raw.push_str(&format!(
        "u UU N... 100644 100644 100644 100644 {} {} {} conflict\0",
        "a".repeat(40),
        "b".repeat(40),
        "c".repeat(40)
    ));
    raw.push_str("? new/é.rs\0# future.extension value\0");
    let parsed = parse(raw.as_bytes()).unwrap();
    assert_eq!(parsed.entries.len(), 4);
    let mixed = parsed
        .entries
        .iter()
        .find(|entry| entry.path.starts_with("mixed"))
        .unwrap();
    assert!(mixed.matches(Kind::Staged) && mixed.matches(Kind::Unstaged));
    let renamed = parsed
        .entries
        .iter()
        .find(|entry| entry.original_path.is_some())
        .unwrap();
    assert_eq!(renamed.original_path.as_deref(), Some("old\tname"));
    assert_eq!(renamed.similarity, Some(100));
    let conflict = &parsed.entries[0];
    assert!(conflict.conflicted);
    assert!(!conflict.matches(Kind::Staged) && !conflict.matches(Kind::Unstaged));
    assert!(conflict.index_status.is_none() && conflict.worktree_status.is_none());
    for length in 1..raw.len() {
        if raw.as_bytes()[length - 1] != 0 {
            assert!(parse(&raw.as_bytes()[..length]).is_err());
        }
    }
}

#[test]
fn rejects_malformed_status_instead_of_returning_clean_or_partial_results() {
    let valid = ordinary("M.", "app.rs");
    let oid = "a".repeat(40);
    for body in [
        "? ../escape\0".into(),
        "? /absolute\0".into(),
        "? \0".into(),
        "! ignored\0".into(),
        "\0".into(),
        valid.replace("M.", "X."),
        valid.replace("M.", "U."),
        valid.replace("M.", ".."),
        valid.replace("100644", "100888"),
        valid.replace(&oid, "bogus"),
        valid.replace("N...", "Sbad"),
        valid.replace("M.", "R."),
        format!("{valid}{valid}"),
        format!("2 R. N... 100644 100644 100644 {oid} {oid} R101 new\0old\0"),
        format!("2 R. N... 100644 100644 100644 {oid} {oid} R100 new\0"),
    ] {
        assert!(
            parse(format!("{}{body}", headers()).as_bytes()).is_err(),
            "{body:?}"
        );
    }
    assert!(parse(b"").is_err());
    assert!(parse(b"? app.rs\0").is_err());
    let mut invalid_utf8 = headers().into_bytes();
    invalid_utf8.extend_from_slice(b"? bad\xff\0");
    assert!(parse(&invalid_utf8).is_err());
}

#[test]
fn sha256_repositories_and_unborn_or_detached_labels_are_preserved() {
    let raw = format!(
        "# branch.oid {}\0# branch.head (detached)\0{}",
        "a".repeat(64),
        ordinary(".M", "app.rs")
            .replace(&"a".repeat(40), &"a".repeat(64))
            .replace(&"b".repeat(40), &"b".repeat(64))
    );
    let parsed = parse(raw.as_bytes()).unwrap();
    assert_eq!(parsed.head.unwrap().len(), 64);
    assert_eq!(parsed.branch, "(detached)");
    let unborn = parse(b"# branch.oid (initial)\0# branch.head main\0").unwrap();
    assert!(unborn.head.is_none());
    assert!(unborn.entries.is_empty());
}
