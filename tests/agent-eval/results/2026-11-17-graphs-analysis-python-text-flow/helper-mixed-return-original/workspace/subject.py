def maybe_text(value):
    if value:
        return str(value)
    return 7
def entry():
    return send(maybe_text(read_user()).lower())
