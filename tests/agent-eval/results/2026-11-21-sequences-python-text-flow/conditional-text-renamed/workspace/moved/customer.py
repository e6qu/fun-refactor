def customer_entry():
    value = str(load_input()) if True else 'PUBLIC'
    return emit(value.lower())
