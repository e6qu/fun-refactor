def normalized(value):
    return str(value).strip()
def customer_entry():
    return emit(normalized(load_input()).lower())
