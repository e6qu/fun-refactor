# expect: passes
# run: yes
# title: The two fields the program actually uses, with types of their own
# improves: deploy_mirror
from dataclasses import dataclass
from typing import NewType

Port = NewType("Port", int)


@dataclass(frozen=True)
class ServerPlan:
    name: str
    port: Port
    replicas: int


def read_plan(attributes: dict[str, object]) -> ServerPlan:
    match attributes:
        case {"name": str(name), "port": int(port), **rest}:
            replicas = rest.get("replicas", 1)
            if not 1 <= port <= 65535 or not isinstance(replicas, int):
                raise ValueError(f"bad plan: {attributes}")
            return ServerPlan(name, Port(port), replicas)
        case _:
            raise ValueError(f"bad plan: {attributes}")


def summary(plan: ServerPlan) -> str:
    return f"{plan.name} on {plan.port} x{plan.replicas}"


assert summary(read_plan({"name": "shop", "port": 8080})) == "shop on 8080 x1"
