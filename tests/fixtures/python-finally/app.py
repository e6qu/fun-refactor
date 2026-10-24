import delivery


def entry():
    return send(delivery.prepare(read_user()))
