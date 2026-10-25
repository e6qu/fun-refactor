import re
def customer_entry():
    return emit(re.sub('A', 'B', str(load_input())))
