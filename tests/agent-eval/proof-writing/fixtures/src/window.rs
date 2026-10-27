pub fn fits_window(offset: u64, count: u64, total: u64) -> bool {
    offset <= total && count <= total - offset
}
