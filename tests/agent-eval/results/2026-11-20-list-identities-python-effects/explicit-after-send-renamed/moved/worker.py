# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    value = load_input()
    emit(value)
    raise ValueError(value)
