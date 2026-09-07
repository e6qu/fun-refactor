use super::*;

fn header(after_mode: &str, width: usize) -> String {
    let old = "a".repeat(width);
    let new = "0".repeat(width);
    format!(":100644 {after_mode} {old} {new} M\0file.txt\0\0diff --git a/file.txt b/file.txt\n")
}

fn patch(body: &str) -> Vec<u8> {
    let header = header("100644", 40);
    format!("{header}--- a/file.txt\n+++ b/file.txt\n{body}").into_bytes()
}

#[test]
fn hunk_coordinates_markers_and_unicode_clipping_preserve_sides() {
    let body = format!("@@ -1,2 +1,2 @@ heading\n same\n-old\n\\ No newline at end of file\n+{}\n\\ No newline at end of file\n@@ -10 +10,0 @@\n-last\n", "名".repeat(500));
    let result = parse(&patch(&body), "file.txt").unwrap();
    assert_eq!((result.hunks, result.added, result.deleted), (2, 1, 2));
    let rows = serde_json::to_value(&result.rows).unwrap();
    assert_eq!(rows[1]["old_line"], 1);
    assert_eq!(rows[1]["new_line"], 1);
    assert_eq!(rows[2]["new_line"], serde_json::Value::Null);
    assert_eq!(rows[3]["old_line"], 2);
    assert_eq!(rows[4]["content"]["bytes"], 1500);
    assert_eq!(rows[4]["content"]["truncated"], true);
    assert_eq!(rows[4]["content"]["text"].as_str().unwrap().len(), 1023);
    assert_eq!(rows[5]["new_line"], 2);
    assert_eq!(rows[7]["old_line"], 10);
}

#[test]
fn malformed_or_incomplete_hunks_refuse_without_partial_rows() {
    for body in [
        "@@ -1 +1 @@\n-old\n",
        "@@ -1 +1 @@\n-old\n+new\n+extra\n",
        "@@ -1 +1 @@\n\\ No newline at end of file\n-old\n+new\n",
        "@@ -1 +1 @@\n-old\n\\ No newline at end of file\n\\ No newline at end of file\n+new\n",
        "@@ -0 +1 @@\n-old\n+new\n",
        "@@ -0,0 +0,0 @@\n",
        "@@ -1 +1 @@bad\n-old\n+new\n",
        "@@ -1 +1 @@\n-old\n@@ -2 +2 @@\n-old\n+new\n",
        "@@ -18446744073709551615 +1 @@\n-old\n+new\n",
        "@@@ -1 -1 +1 @@@\n-old\n+new\n",
    ] {
        assert!(parse(&patch(body), "file.txt").is_err(), "{body:?}");
    }
    let valid = patch("@@ -1 +1 @@\n-old\n+new\n");
    assert!(parse(&valid[..valid.len() - 1], "file.txt").is_err());
    assert!(parse(&valid, "other.txt").is_err());
    let mut invalid = patch("@@ -1 +1 @@\n-old\n+");
    invalid.extend_from_slice(&[255, b'\n']);
    assert!(parse(&invalid, "file.txt").is_err());
}

#[test]
fn metadata_only_binary_and_empty_changes_are_distinct() {
    assert!(parse(b"", "file.txt").unwrap().change.is_none());
    let stat_only = header("100644", 40);
    let stat_only = stat_only.split_once("diff --git").unwrap().0;
    let result = parse(stat_only.as_bytes(), "file.txt").unwrap();
    assert!(result.change.is_none() && result.rows.is_empty());
    assert!(parse(
        stat_only
            .replace("100644 100644", "100644 100755")
            .as_bytes(),
        "file.txt"
    )
    .is_err());
    let mode = header("100755", 64) + "old mode 100644\nnew mode 100755\n";
    let result = parse(mode.as_bytes(), "file.txt").unwrap();
    assert!(result.change.is_some() && result.rows.is_empty() && !result.binary);
    let binary = header("100644", 40) + "Binary files a/file.txt and b/file.txt differ\n";
    assert!(parse(binary.as_bytes(), "file.txt").unwrap().binary);
    for broken in [
        mode.replace("100644", "120000"),
        mode.replace(" M\0", " U\0"),
        mode.replace("\0\0diff", "\0:extra\0\0diff"),
    ] {
        assert!(parse(broken.as_bytes(), "file.txt").is_err());
    }
}

#[test]
fn full_patch_identities_must_agree_with_raw_metadata() {
    let raw = header("100644", 40);
    let old = "a".repeat(40);
    let new = "b".repeat(40);
    let line = format!("index {old}..{new} 100644\n");
    let body = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n";
    let valid = format!("{raw}{line}{body}");
    let value = parse(valid.as_bytes(), "file.txt").unwrap();
    assert_eq!(value.blobs, Some((Some(old.clone()), Some(new))));
    for invalid in [
        format!("{raw}{line}{line}{body}"),
        valid.replace(&line, &line.replace(&old, &"c".repeat(40))),
        valid.replacen(&"0".repeat(40), &"d".repeat(40), 1),
    ] {
        assert!(parse(invalid.as_bytes(), "file.txt").is_err());
    }
}
