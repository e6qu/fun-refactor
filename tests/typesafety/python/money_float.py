# expect: passes
# run: yes
# title: Float arithmetic rounds the cents
"""Ten cents, three times, is thirty cents. The float sum misses it."""


def total_price(prices: list[float]) -> float:
    return sum(prices)


# Three items at ten cents each. CI runs this line.
assert total_price([0.1, 0.1, 0.1]) != 0.3
