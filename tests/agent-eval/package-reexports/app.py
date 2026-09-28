from portal import dispatch
import archive.transform as fixed


def render():
    return dispatch(source())


def safe():
    return sink(fixed.clean_value(source()))
