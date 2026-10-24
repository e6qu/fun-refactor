# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    values = [read_user()]
    alias = values
    alias[0] = 0
    return send(values[0])
