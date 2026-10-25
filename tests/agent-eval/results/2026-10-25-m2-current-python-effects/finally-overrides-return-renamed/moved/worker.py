# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    try:
        return emit(load_input())
    finally:
        return 0
