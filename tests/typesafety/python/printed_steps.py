# expect: passes
# title: The steps print their trail, so the trail escapes the program
"""Each step logs by printing. The order of lines depends on when the steps
run, a test has to capture stdout to see them, and a caller can neither
inspect the trail nor attach it to the answer."""


def double(n: int) -> int:
    print(f"doubled {n}")
    return n * 2


def add_tax(n: int) -> int:
    print(f"taxed {n}")
    return n + n // 10


def total(n: int) -> int:
    return add_tax(double(n))
