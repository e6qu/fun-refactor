# expect: passes
# run: yes
# title: Adding 0.1 three times does not equal 0.3
def total_price(prices: list[float]) -> float:
    return sum(prices)


# Three items at ten cents each. CI runs this line.
assert total_price([0.1, 0.1, 0.1]) != 0.3
