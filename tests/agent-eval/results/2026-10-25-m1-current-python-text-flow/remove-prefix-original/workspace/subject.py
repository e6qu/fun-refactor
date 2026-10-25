def entry():
    return send(str(read_user()).removeprefix('customer:'))
