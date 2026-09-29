from .transform import clean_value

def send(value, /, *, spare=None):
    return sink(clean_value(value=value))
