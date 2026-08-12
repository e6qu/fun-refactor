# expect: passes
# run: yes
# title: Integer cents keep every total exact
# improves: money_float
"""The same prices, held as integer cents. The sum is exact at any scale."""


def total_cents(prices_cents: list[int]) -> int:
    return sum(prices_cents)


# The same three items. CI runs this line.
assert total_cents([10, 10, 10]) == 30
