//! Small total policies shared with the Lean transaction model.

/// Whether a browser history record may make the requested stack transition.
///
/// Status codes are applied `0`, undone `1`, abandoned `2`; action codes are undo `0`
/// and redo `1`. Unknown codes refuse.
pub fn memory_transition_allowed(status: usize, action: usize, at_stack_top: bool) -> bool {
    at_stack_top && matches!((status, action), (0, 0) | (1, 1))
}
