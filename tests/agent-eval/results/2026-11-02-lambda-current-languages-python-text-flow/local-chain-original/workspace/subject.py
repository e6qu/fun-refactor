def entry():
    label = str(read_user())
    label = label.strip().upper()
    return send(label)
