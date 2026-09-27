mod pricing;

pub fn quote(a: i64, b: i64) -> i64 {
    pricing::subtotal(a, b)
}
