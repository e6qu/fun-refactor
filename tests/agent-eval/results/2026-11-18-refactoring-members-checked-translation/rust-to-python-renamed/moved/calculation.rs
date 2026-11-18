pub fn total_pages(items: i64, size: i64) -> i64 {
    if items < 0 || size <= 0 {
        return -1;
    }
    let quotient = items / size;
    if items % size == 0 {
        return quotient;
    }
    return quotient + 1;
}
