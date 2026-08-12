# expect: passes
# title: Exercise 4, starting point
"""An amount is a float and its currency is a string beside it. Mixing
currencies is caught at run time, at best."""


def add_prices(
    amount_a: float, currency_a: str, amount_b: float, currency_b: str
) -> float:
    if currency_a != currency_b:
        raise ValueError("cannot add different currencies")
    return amount_a + amount_b


def basket_total() -> float:
    # Raises at run time. Nothing warned about it earlier.
    return add_prices(19.99, "USD", 5.00, "EUR")
