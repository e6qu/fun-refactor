# expect: fails
# title: The raw settings dictionary where a Config is required, rejected by the checker
# misuse-of: config_parse_once
from dataclasses import dataclass


@dataclass(frozen=True)
class Config:
    port: int
    verbose: bool


def connect(config: Config) -> str:
    return f"connecting on {config.port}"


greeting = connect({"port": "8080", "verbose": "true"})
