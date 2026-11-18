def entry():
    if read_user():
        label = 'FIRST'
    else:
        label = 'SECOND'
    return send(label.lower())
