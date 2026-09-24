def relay(value, count):
    if count:
        return relay(value, count - 1)
    return value


def checkout(count):
    first = café(); second = café()
    sink(relay(first, count))
    sink(second)


def overwritten():
    value = café()
    value = 0
    sink(value)


def normalized(value):
    return value % 2


def normalization_boundary():
    sink(normalized(café()))


def unknown(value):
    return external(value)


def sanitized():
    sink(clean(café()))
