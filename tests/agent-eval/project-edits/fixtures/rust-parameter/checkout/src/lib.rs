use pricing::total;

// Preserve the imported caller, Unicode π, and surrounding source.
pub fn charge(amount: i64) -> i64 {
    total(amount, false, 3)
}

pub fn combined(amount: i64) -> i64 {
    pricing::same_crate(amount) + total(amount, true, 4)
}
