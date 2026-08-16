# expect: fails
# title: The attribute bag where a ServerPlan is required, rejected by the checker
# misuse-of: deploy_plan
from dataclasses import dataclass
from typing import NewType

type Json = None | bool | int | float | str | list["Json"] | dict[str, "Json"]

Port = NewType("Port", int)


@dataclass(frozen=True)
class Resource:
    kind: str
    name: str
    attributes: dict[str, Json]


@dataclass(frozen=True)
class ServerPlan:
    name: str
    port: Port
    replicas: int


def summary(plan: ServerPlan) -> str:
    return f"{plan.name} on {plan.port} x{plan.replicas}"


def report(resource: Resource) -> str:
    return summary(resource)
