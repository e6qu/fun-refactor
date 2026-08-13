# expect: passes
# run: yes
# title: Integer cents add up exactly
# improves: money_float
def total_cents(prices_cents: list[int]) -> int:
    return sum(prices_cents)


# The same three items. CI runs this line.
assert total_cents([10, 10, 10]) == 30
