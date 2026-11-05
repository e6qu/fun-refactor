def entry():
    first, second = str(read_user()), 7
    first, second = second, first
    return send(second.lower())
