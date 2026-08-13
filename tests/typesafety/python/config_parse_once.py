# expect: passes
# title: parse_argv checks the port once, and Config carries the proof
# improves: config_validate_everywhere
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
    port = int(port_text)
    if not 1 <= port <= 65535:
        raise ValueError("port out of range")
    verbose_text = settings.get("verbose", "false")
    if verbose_text not in ("true", "false"):
        raise ValueError("verbose must be true or false")
    return Config(port=port, verbose=verbose_text == "true")


def connect(config: Config) -> str:
    return f"connecting on {config.port}"


def report(config: Config) -> str:
    return f"listening on {config.port}"
