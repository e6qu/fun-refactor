def normalized(value, again):
    if again:
        return normalized(value, False)
    return str(value)
def entry():
    return send(normalized(read_user(), True).lower())
