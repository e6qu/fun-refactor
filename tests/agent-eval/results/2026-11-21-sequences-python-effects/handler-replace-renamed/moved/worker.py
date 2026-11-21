# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    try:
        value = load_input()
        1 // 0
    except ZeroDivisionError:
        value = 0
    return emit(value)
