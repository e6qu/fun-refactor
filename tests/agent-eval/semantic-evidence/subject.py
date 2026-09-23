def café(value):
    return value


def total(amount):
    """Keep repeated calls distinct despite identical lowered expressions."""
    result = café(amount) + café(amount)
    result += 1
    if amount:
        result = result + 2
    else:
        pass
    return result
