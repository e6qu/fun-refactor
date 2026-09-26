from .transform import clean_value


def send(value):
    return sink(clean_value(value))
