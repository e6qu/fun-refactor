from portal import transport as api
import archive.transform as fixed


def render():
    return api.send(source())


def safe():
    return sink(fixed.clean_value(source()))
