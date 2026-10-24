# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    try:
        value = read_user()
        1 // 0
    except ZeroDivisionError:
        value = 0
    return send(value)
