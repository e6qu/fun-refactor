def entry():
    label = str(read_user())
    label = 'PUBLIC'
    return send(label.lower())
