def normalized(value, again):
    if again:
        return normalized(value, False)
    return str(value)
def customer_entry():
    return emit(normalized(load_input(), True).lower())
