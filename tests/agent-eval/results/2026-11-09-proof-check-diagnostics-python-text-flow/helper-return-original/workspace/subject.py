def normalized(value):
    return str(value).strip()
def entry():
    return send(normalized(read_user()).lower())
