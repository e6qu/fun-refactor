use super::*;

fn raw(before: &str, after: &str, state: &str, path: &str, width: usize) -> String {
    let old = if before == "000000" { "0" } else { "a" }.repeat(width);
    let new = if after == "000000" { "0" } else { "b" }.repeat(width);
    format!(":{before} {after} {old} {new} {state}\0{path}\0")
}

#[test]
fn joins_raw_and_numeric_records_without_losing_path_bytes() {
    let unusual = "z\t\né";
    let input = raw("100644", "100755", "M", unusual, 64)
        + &raw("000000", "100644", "A", "added", 64)
        + &raw("100644", "000000", "D", "gone", 64)
        + &format!("-\t-\t{unusual}\0")
        + "0\t0\tadded\0"
        + "0\t4\tgone\0";
    let rows = parse(input.as_bytes()).unwrap();
    assert_eq!(
        rows.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(),
        ["added", "gone", unusual]
    );
    assert!(rows[2].binary && rows[2].added_lines.is_none());
    assert_eq!(rows[1].deleted_lines, Some(4));
    assert_eq!(rows[0].before_oid, None);
    assert!(rows.iter().all(|r| r.detail_candidate));
}

#[test]
fn drops_stat_only_records_and_submodules_but_keeps_mode_only_changes() {
    let stat = raw("100644", "100644", "M", "stat", 40).replace(&"b".repeat(40), &"0".repeat(40));
    assert!(parse(stat.as_bytes()).unwrap().is_empty());
    let input = raw("100644", "100755", "M", "mode", 40)
        + &raw("160000", "160000", "M", "sub", 40)
        + "0\t0\tmode\0"
        + "1\t1\tsub\0";
    let rows = parse(input.as_bytes()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, "mode");
    assert!(parse(b"").unwrap().is_empty());
}

#[test]
fn rejects_malformed_inconsistent_and_ambiguous_reports() {
    let head = raw("100644", "100644", "M", "file", 40);
    let valid = head.clone() + "1\t1\tfile\0";
    for invalid in [
        head.clone(),
        valid.trim_end_matches('\0').into(),
        head.clone() + "-\t1\tfile\0",
        head.clone() + "+1\t1\tfile\0",
        head.clone() + "18446744073709551616\t1\tfile\0",
        head.clone() + "1\t1\tother\0",
        head.clone() + "1\t1\t\0",
        head.clone() + &head + "1\t1\tfile\0",
        valid.clone() + "1\t1\tfile\0",
        valid.replace(" M\0", " U\0"),
        valid.replace(" M\0", " T\0"),
        valid.replace(" M\0", " R100\0"),
        valid.replace("100644", "777777"),
        valid.replace("file", "../outside"),
        valid.replace("file", "/absolute"),
        valid.replace(&"a".repeat(40), "bad"),
        raw("000000", "100644", "M", "file", 40) + "1\t0\tfile\0",
    ] {
        assert!(parse(invalid.as_bytes()).is_err(), "{invalid:?}");
    }
    let mut invalid = valid.into_bytes();
    invalid[0] = 255;
    assert!(parse(&invalid).is_err());
}
