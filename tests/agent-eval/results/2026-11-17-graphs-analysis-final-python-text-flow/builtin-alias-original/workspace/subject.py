from builtins import str as text
def entry():
    return send(text(read_user()).lower())
