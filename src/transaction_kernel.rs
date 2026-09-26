/// Whether a browser history record may make the requested stack transition.
pub fn memory_transition_allowed(status: usize, action: usize, at_stack_top: bool) -> bool {
    at_stack_top && matches!((status, action), (0, 0) | (1, 1))
}

pub fn memory_restore_allowed(
    schema_matches: bool,
    digest_matches: bool,
    history_valid: bool,
    files: usize,
    payload_bytes: usize,
) -> bool {
    schema_matches
        && digest_matches
        && history_valid
        && files <= 4096
        && payload_bytes <= 4 * 1024 * 1024
}

pub fn memory_compaction_allowed(keep: usize) -> bool {
    keep <= 256
}

pub fn history_publication_allowed(target_matches: bool, staged_matches: bool) -> bool {
    target_matches && staged_matches
}

pub fn history_recovery_step(matches_before: bool, matches_after: bool) -> usize {
    if matches_before {
        0
    } else if matches_after {
        1
    } else {
        2
    }
}
