pub fn width(value: &str) -> usize {
    #[cfg(fr_strict)]
    { value }
    #[cfg(not(fr_strict))]
    { value.len() }
}

pub fn repeated() -> usize {
    let café = "hello";
    width(café) + width(café)
}
