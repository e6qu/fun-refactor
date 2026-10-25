# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    values = [0]
    alias = values
    alias[0] = read_user()
    return send(values[0])
