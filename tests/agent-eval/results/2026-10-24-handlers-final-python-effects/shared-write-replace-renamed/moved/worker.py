# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    values = [load_input()]
    alias = values
    alias[0] = 0
    return emit(values[0])
