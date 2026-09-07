use super::*;

fn record(fields: &[&str]) -> Vec<u8> {
    format!("{}\0\0", fields.join("\0")).into_bytes()
}

#[test]
fn preserves_nul_delimited_paths_and_caps_multibyte_reasons() {
    let root = Path::new("/root\nwith space");
    let text = format!("{}\nlast", "é".repeat(300));
    let bytes = record(&[
        "worktree /root\nwith space",
        &format!("HEAD {}", "a".repeat(64)),
        "detached",
        &format!("locked {text}"),
        "prunable",
    ]);
    let rows = parse(&bytes, root).unwrap();
    assert_eq!(rows[0].path, root.to_str().unwrap());
    assert!(rows[0].current && rows[0].main && rows[0].detached);
    assert_eq!(rows[0].lock_reason.as_ref().unwrap().len(), 512);
    assert!(rows[0].lock_reason_truncated && rows[0].prunable);
    assert_eq!(rows[0].prune_reason, None);
}

#[test]
fn bare_primary_and_unborn_linked_metadata_are_distinct() {
    let mut bytes = record(&["worktree /bare", "bare"]);
    bytes.extend(record(&[
        "worktree /root",
        &format!("HEAD {}", "0".repeat(40)),
        "branch refs/heads/main",
        "locked",
    ]));
    let rows = parse(&bytes, Path::new("/root")).unwrap();
    assert!(rows[0].bare && rows[0].main && !rows[0].unborn);
    assert!(rows[1].current && rows[1].unborn && rows[1].locked);
    assert_eq!(rows[1].head, None);
    assert_eq!(rows[1].lock_reason, None);
}

#[test]
fn refuses_ambiguous_or_incomplete_records() {
    let head = format!("HEAD {}", "a".repeat(40));
    let valid = ["worktree /root", &head, "branch refs/heads/main"];
    let cases: Vec<Vec<&str>> = vec![
        vec![],
        vec!["bare"],
        vec!["worktree /root"],
        vec!["worktree relative", &head, "detached"],
        vec!["worktree /root", "HEAD invalid", "detached"],
        vec!["worktree /root", &head],
        vec![
            "worktree /root",
            &head,
            "detached",
            "branch refs/heads/main",
        ],
        vec!["worktree /root", &head, "bare"],
        vec![
            "worktree /root",
            &head,
            "detached",
            "locked",
            "locked reason",
        ],
        vec!["worktree /root", &head, "detached", "future-attribute"],
        vec!["worktree /root", &head, "detached yes"],
        vec!["worktree /root", &head, "branch refs/tags/tag"],
        vec!["worktree /root", &head, "branch refs/heads/"],
        vec!["worktree /root", &head, "detached", "worktree /other"],
    ];
    for fields in cases {
        assert!(
            parse(&record(&fields), Path::new("/root")).is_err(),
            "{fields:?}"
        );
    }
    let bytes = record(&valid);
    for end in 0..bytes.len() {
        assert!(parse(&bytes[..end], Path::new("/root")).is_err());
    }
    let mut duplicate = bytes.clone();
    duplicate.extend(&bytes);
    assert!(parse(&duplicate, Path::new("/root")).is_err());
    assert!(parse(&bytes, Path::new("/elsewhere")).is_err());
    assert!(parse(b"worktree /root\xff\0bare\0\0", Path::new("/root")).is_err());
    assert!(parse(
        &record(&[
            "worktree /root",
            &format!("HEAD {}", "0".repeat(64)),
            "detached"
        ]),
        Path::new("/root")
    )
    .is_err());
}
