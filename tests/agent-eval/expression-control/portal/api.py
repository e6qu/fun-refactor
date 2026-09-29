from .transform import clean_value


def send(value):
    return (False and sink(value)) or (sink(clean_value(value)) if True else sink(value))
