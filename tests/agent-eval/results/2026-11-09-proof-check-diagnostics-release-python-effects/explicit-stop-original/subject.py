# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    value = read_user()
    raise ValueError(value)
    send(value)
