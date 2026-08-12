# expect: passes
# run: yes
# title: A type that cannot hold the values you meant
"""`float` cannot represent 0.1 exactly. Integer cents can represent every price."""


def total_as_float(prices: list[float]) -> float:
    return sum(prices)


def total_as_cents(prices_cents: list[int]) -> int:
    return sum(prices_cents)


# Three items at ten cents each. CI runs these lines.
assert total_as_float([0.1, 0.1, 0.1]) != 0.3
assert total_as_cents([10, 10, 10]) == 30
