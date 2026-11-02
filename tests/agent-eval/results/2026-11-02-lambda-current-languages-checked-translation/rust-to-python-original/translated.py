# Translated from rust (/home/runner/work/_temp/language-behavior/translation/rust-to-python-original/calculation.rs) by fun-refactor.
# 1 function(s), 0 record(s), 0 constant(s).
# Every signature carried across with its types intact.

def fr_trunc_rem(dividend: int, divisor: int) -> int:
    """The remainder that goes with division truncating toward zero.

    Python's own `%` rounds with `//`, toward negative infinity, and the
    two answer differently whenever the operands have different signs.
    """
    remainder = dividend % divisor
    if remainder != 0 and (remainder < 0) != (dividend < 0):
        return remainder - divisor
    return remainder

def page_count(items: int, size: int) -> int:
    if items < 0 or size <= 0:
        return -1
    quotient = (lambda dividend, divisor: dividend // divisor + (1 if (dividend < 0) != (divisor < 0) and dividend % divisor != 0 else 0))(items, size)
    if fr_trunc_rem(items, size) == 0:
        return quotient
    return quotient + 1

