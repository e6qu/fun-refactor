def delayed(count):
    first = 0
    second = 0
    while count:
        sink(second)
        second = first
        first = source()
        count = count - 1


def overwritten(count):
    value = source()
    while count:
        value = 0
        sink(value)
        count = count - 1


def stopped(count):
    while count:
        break
        sink(source())


def skipped(count):
    while count:
        count = count - 1
        continue
        sink(source())


def raised(error):
    raise error
    sink(source())


def recursive(value):
    return recursive(value)
