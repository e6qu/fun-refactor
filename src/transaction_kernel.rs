/// Whether a browser history record may make the requested stack transition.
pub fn memory_transition_allowed(status: usize, action: usize, at_stack_top: bool) -> bool {
    at_stack_top && matches!((status, action), (0, 0) | (1, 1))
}
