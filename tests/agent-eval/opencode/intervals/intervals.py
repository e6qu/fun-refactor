"""Operations on half-open integer intervals."""


def overlaps(left, right):
    """Return whether two intervals share an integer point."""
    return max(left[0], right[0]) <= min(left[1], right[1])


def span(interval):
    """Return the nonnegative length of an interval."""
    return max(0, interval[1] - interval[0])
