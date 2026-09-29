from .transform import clean_value

def send(value, /, *, spare):
    return sink(clean_value(value=value))
