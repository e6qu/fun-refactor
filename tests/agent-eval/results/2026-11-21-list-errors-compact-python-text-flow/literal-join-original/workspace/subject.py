def entry():
    value = str(read_user()) + '!'
    return send(value.lower())
