# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    try:
        1 // 0
    except ZeroDivisionError:
        return emit(load_input())
