// Prices are integer cents. Keep this café comment and its spacing.
pub fn total(amount: i64, _verbose: bool, fee: i64) -> i64 {
    amount + fee
}

pub fn same_crate(amount: i64) -> i64 {
    total(amount, true, 2)
}

// This unrelated function and its formatting must stay intact.
pub fn unrelated(amount: i64) -> i64 { amount + 101 }
