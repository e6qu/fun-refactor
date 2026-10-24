from boltons.mathutils import clamp


def prepare(value, lower, upper):
    try:
        clamp(0, lower, upper)
    except TypeError:
        return value
    return 'ACCEPTED'


def entry():
    return send(prepare(read_user(), 50, 0))
