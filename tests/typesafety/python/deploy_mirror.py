# expect: passes
# title: The config file's grammar, copied into types that know nothing
from dataclasses import dataclass
from typing import cast

type Json = None | bool | int | float | str | list["Json"] | dict[str, "Json"]


@dataclass(frozen=True)
class Resource:
    kind: str
    name: str
    attributes: dict[str, Json]


def port_of(resource: Resource) -> int:
    return cast(int, resource.attributes["port"])


def replicas_of(resource: Resource) -> int:
    return cast(int, resource.attributes.get("replicas", 1))


def summary(resource: Resource) -> str:
    return f"{resource.name} on {port_of(resource)} x{replicas_of(resource)}"
