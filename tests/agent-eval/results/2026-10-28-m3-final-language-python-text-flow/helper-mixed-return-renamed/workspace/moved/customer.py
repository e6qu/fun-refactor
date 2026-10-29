def maybe_text(value):
    if value:
        return str(value)
    return 7
def customer_entry():
    return emit(maybe_text(load_input()).lower())
