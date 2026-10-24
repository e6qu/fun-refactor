# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    values = []
    alias = values
    alias.append(load_input())
    return emit(values[0])
