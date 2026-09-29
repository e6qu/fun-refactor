from portal.api import send as dispatch
import archive.transform as fixed


def render():
    return dispatch(source(), spare=0)


def safe():
    return sink(fixed.clean_value(source()))
