from __future__ import annotations
def normalized(value: str):
    return value.lower()
def customer_entry():
    return emit(normalized(load_input()))
