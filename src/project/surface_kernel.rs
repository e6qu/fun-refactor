pub fn style_literal_resolution(definition_count: usize, tailwind_context: bool) -> usize {
    if definition_count == 1 {
        0
    } else if definition_count > 1 {
        1
    } else if tailwind_context {
        2
    } else {
        3
    }
}

pub fn surface_items_emitted(total: usize, limit: usize) -> usize {
    total.min(limit)
}

pub fn surface_items_omitted(total: usize, limit: usize) -> usize {
    total.saturating_sub(limit)
}
