from __future__ import annotations
def normalized(value: str):
    return value.lower()
def entry():
    return send(normalized(read_user()))
