// Translated from python (/home/runner/work/_temp/language-behavior/translation/python-to-rust-original/calculation.py) by fun-refactor.
// 1 function(s), 0 record(s), 0 constant(s).
// Every signature carried across with its types intact.

pub fn page_count(items: i64, size: i64) -> i64 {
    if items < 0 || size <= 0 {
        return -1;
    }
    let mut quotient = floor_div(items, size);
    if fr_floor_rem(items, size) == 0 {
        return quotient;
    }
    return quotient + 1;
}

/// The remainder that goes with division rounding toward negative
/// infinity: `%` truncates and `rem_euclid` never goes negative.
fn fr_floor_rem(dividend: i64, divisor: i64) -> i64 {
    let remainder = dividend % divisor;
    match remainder != 0 && (remainder < 0) != (divisor < 0) {
        true => remainder + divisor,
        false => remainder,
    }
}

/// Division that rounds toward negative infinity.
///
/// The standard library's Euclidean division keeps the remainder
/// positive, which is another answer when the divisor is negative.
fn floor_div(dividend: i64, divisor: i64) -> i64 {
    let quotient = dividend / divisor;
    let remainder = dividend % divisor;
    match remainder != 0 && (remainder < 0) != (divisor < 0) {
        true => quotient - 1,
        false => quotient,
    }
}

