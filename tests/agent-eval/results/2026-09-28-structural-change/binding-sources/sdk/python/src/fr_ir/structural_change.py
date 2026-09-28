"""Typed requests for reviewed, consumer-bound Rust structural changes."""
from __future__ import annotations

from dataclasses import dataclass
import json
from typing import Any, Literal

from .ir import IrError, TaskTarget


@dataclass(frozen=True)
class RefactorRequest:
    operation: Literal["rename", "remove-parameter", "move-parameter"]
    value: str | int
    destination: int | None = None

    def __post_init__(self) -> None:
        if self.operation == "rename":
            if (not isinstance(self.value, str) or not self.value
                    or len(self.value.encode()) > 256 or "\0" in self.value
                    or self.destination is not None):
                raise IrError("rename requires a bounded name; native preflight checks Rust identifiers")
        elif self.operation in ("remove-parameter", "move-parameter"):
            if type(self.value) is not int or not 0 <= self.value < 64:
                raise IrError("parameter position must be an integer below 64")
            if self.operation == "remove-parameter" and self.destination is not None:
                raise IrError("parameter removal has no destination")
            if self.operation == "move-parameter" and (
                    type(self.destination) is not int or not 0 <= self.destination < 64
                    or self.destination == self.value):
                raise IrError("parameter movement needs a distinct integer destination below 64")
        else:
            raise IrError("unsupported structural refactor")

    @classmethod
    def rename(cls, name: str) -> RefactorRequest:
        return cls("rename", name)

    @classmethod
    def remove_parameter(cls, index: int) -> RefactorRequest:
        return cls("remove-parameter", index)

    @classmethod
    def move_parameter(cls, source: int, destination: int) -> RefactorRequest:
        return cls("move-parameter", source, destination)

    def to_data(self) -> dict[str, Any]:
        self.__post_init__()
        if self.operation == "rename":
            return {"operation": self.operation, "name": self.value}
        if self.operation == "remove-parameter":
            return {"operation": self.operation, "index": self.value}
        return {"operation": self.operation, "from": self.value, "to": self.destination}

    def target(self, id: str, handle: str) -> TaskTarget:
        return TaskTarget(id, handle, "refactor", fragment=json.dumps(self.to_data(), separators=(",", ":")))
