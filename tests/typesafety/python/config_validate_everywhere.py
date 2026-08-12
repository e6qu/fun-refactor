# expect: passes
# title: The same check, repeated by every reader
"""Settings travel as a dict of strings. Every reader re-checks the port,
because no reader can trust that another already did."""


def read_argv(argv: list[str]) -> dict[str, str]:
    settings: dict[str, str] = {}
    for pair in argv:
        key, _, value = pair.partition("=")
        settings[key] = value
    return settings


def connect(settings: dict[str, str]) -> str:
    port_text = settings.get("port")
    if port_text is None or not port_text.isdigit():
        raise ValueError("port missing or not a number")
    return f"connecting on {int(port_text)}"


def report(settings: dict[str, str]) -> str:
    port_text = settings.get("port")
    if port_text is None or not port_text.isdigit():  # the same check, again
        raise ValueError("port missing or not a number")
    return f"listening on {int(port_text)}"
