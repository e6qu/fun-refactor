def permits(b: bool, a: bool) -> bool:
    return a and not b


def restricted(a: bool, b: bool, extra: bool) -> bool:
    return a and not b and extra
