def customer_entry():
    if load_input():
        label = 'FIRST'
    else:
        label = 'SECOND'
    return emit(label.lower())
