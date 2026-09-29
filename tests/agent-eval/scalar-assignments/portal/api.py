from .transform import clean_value


def send(value):
    current, spare = clean_value(value), 0
    current, spare = spare, current
    observed = forwarded = spare
    return sink(forwarded)
