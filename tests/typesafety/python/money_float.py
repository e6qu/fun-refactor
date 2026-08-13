# expect: passes
# run: yes
# title: Three parts at two shillings do not come to six shillings
def total_pounds(prices_pounds: list[float]) -> float:
    return sum(prices_pounds)


assert total_pounds([0.1, 0.1, 0.1]) != 0.3
