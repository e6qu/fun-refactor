def entry():
    return send(str(read_user()).replace('x', 'y', -1))
