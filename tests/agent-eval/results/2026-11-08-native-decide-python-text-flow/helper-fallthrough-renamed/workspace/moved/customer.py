def maybe_text(value):
    if value:
        return str(value)
def customer_entry():
    return emit(maybe_text(load_input()).lower())
