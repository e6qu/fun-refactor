# expect: passes
# run: yes
# title: Integer pence add up exactly, and carry into shillings and pounds
# improves: money_float
def total_pence(prices_pence: list[int]) -> int:
    return sum(prices_pence)


def show(pence: int) -> str:
    shillings, d = divmod(pence, 12)
    pounds, s = divmod(shillings, 20)
    return f"£{pounds} {s}s {d}d"


assert total_pence([24, 24, 24]) == 72
assert show(72) == "£0 6s 0d"
