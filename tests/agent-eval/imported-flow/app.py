from relay import forward as relay
import erase as cleaned
from effects import emit


def positive():
    value = source()
    return sink(relay(value))


def negative():
    return sink(cleaned.forward(source()))


def effects():
    return emit(source())


def contextual():
    return sink(relay(clean(source())))


def separate():
    relay(source())
    return sink(relay(0))
