# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    value = read_user()
    send(value)
    raise ValueError(value)
