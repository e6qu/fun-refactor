# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    values = []
    alias = values
    alias.append(read_user())
    return send(values[0])
