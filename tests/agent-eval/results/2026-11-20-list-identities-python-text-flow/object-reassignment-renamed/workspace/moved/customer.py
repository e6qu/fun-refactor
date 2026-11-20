class Other:
    def lower(self):
        return 0
def customer_entry():
    label = str(load_input())
    label = Other()
    return emit(label.lower())
