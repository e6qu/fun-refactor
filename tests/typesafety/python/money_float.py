# expect: passes
# run: yes
# title: Three parts at two shillings do not come to six shillings
def total_pounds(prices_pounds: list[float]) -> float:
    return sum(prices_pounds)


# Two shillings is a tenth of a pound. CI runs this line.
assert total_pounds([0.1, 0.1, 0.1]) != 0.3
