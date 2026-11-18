# A café keeps UTF-8 byte offsets distinct from character columns.
def entry():
    try:
        return send(read_user())
    finally:
        return 0
