def customer_entry():
    first, second = str(load_input()), 7
    first, second = second, first
    return emit(second.lower())
