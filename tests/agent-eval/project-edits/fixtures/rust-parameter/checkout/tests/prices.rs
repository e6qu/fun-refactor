#[test]
fn checkout_prices_stay_the_same() {
    assert_eq!(checkout::charge(5), 8);
    assert_eq!(checkout::combined(5), 16);
    assert_eq!(checkout::charge(-5), -2);
}
