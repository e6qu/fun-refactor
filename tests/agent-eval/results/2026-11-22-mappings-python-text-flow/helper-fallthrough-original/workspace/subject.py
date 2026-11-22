def maybe_text(value):
    if value:
        return str(value)
def entry():
    return send(maybe_text(read_user()).lower())
