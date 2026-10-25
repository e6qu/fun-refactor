#[test]
fn public_prices_stay_the_same() {
    assert_eq!(pricing::total(5, true, 2), 7);
    assert_eq!(pricing::total(-5, false, 2), -3);
    assert_eq!(pricing::same_crate(5), 7);
    assert_eq!(pricing::unrelated(5), 106);
}
