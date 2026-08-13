# expect: passes
# title: The steps print their log, and the caller cannot read it
def double(n: int) -> int:
    print(f"doubled {n}")
    return n * 2


def add_tax(n: int) -> int:
    print(f"taxed {n}")
    return n + n // 10


def total(n: int) -> int:
    return add_tax(double(n))
