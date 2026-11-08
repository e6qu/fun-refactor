def entry():
    value = str(read_user()) if True else 'PUBLIC'
    return send(value.lower())
