def relay(value, count):
    if count:
        return relay(value, count - 1)
    return value


def positive(count):
    sink(relay(source(), count))


def erase(value, count):
    if count:
        return erase(0, count - 1)
    return 0


def negative(count):
    sink(erase(source(), count))


def even(value, count):
    if count:
        return odd(value, count - 1)
    return value


def odd(value, count):
    if count:
        return even(value, count - 1)
    return value


def mutual(count):
    sink(even(source(), count))


def emit(value, count):
    if count:
        emit(value, count - 1)
    sink(value)


def effects(count):
    emit(source(), count)


def forever(value):
    return forever(value)


def unreachable():
    forever(source())
    sink(source())


def throw(value):
    raise value


def exceptional(error):
    throw(error)
    sink(source())


def sanitized(count):
    sink(relay(clean(source()), count))


def separate(count):
    relay(source(), count)
    sink(relay(0, count))


def unknown(value):
    return external(value)


def aliased(value):
    value[0] = source()
    sink(value[0])
