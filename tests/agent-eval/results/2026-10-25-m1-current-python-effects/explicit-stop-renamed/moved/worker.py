# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    value = load_input()
    raise ValueError(value)
    emit(value)
