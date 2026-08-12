# expect: passes
# title: Parse the command line once
# improves: config_validate_everywhere
"""One function turns the argument strings into a Config, or raises. Past that
point `port` exists and is a number, and the type says so."""

from dataclasses import dataclass


@dataclass(frozen=True)
class Config:
    port: int
    verbose: bool


def parse_argv(argv: list[str]) -> Config:
    settings: dict[str, str] = {}
    for pair in argv:
        key, _, value = pair.partition("=")
        settings[key] = value
    port_text = settings.get("port")
    if port_text is None or not port_text.isdigit():
        raise ValueError("port missing or not a number")
    return Config(port=int(port_text), verbose=settings.get("verbose") == "true")


def connect(config: Config) -> str:
    return f"connecting on {config.port}"  # no check, and none needed


def report(config: Config) -> str:
    return f"listening on {config.port}"
