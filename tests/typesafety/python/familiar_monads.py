# expect: passes
# run: yes
# title: Three you already use: a comprehension, await, and a value that may be missing
from collections.abc import Awaitable, Callable
from dataclasses import dataclass


@dataclass(frozen=True)
class Assembly:
    name: str
    parts: tuple[str, ...]


def all_parts(assemblies: list[Assembly]) -> list[str]:
    return [part for assembly in assemblies for part in assembly.parts]


async def quoted_total(fetch: Callable[[str], Awaitable[int]]) -> int:
    frame = await fetch("F-101")
    wheels = await fetch("W-200")
    return frame + wheels


def note_length(note: str | None) -> int:
    if note is None:
        return 0
    return len(note)


assemblies = [Assembly("frame", ("top tube", "down tube")), Assembly("wheel", ("rim",))]
assert all_parts(assemblies) == ["top tube", "down tube", "rim"]
assert note_length("fragile") == 7
assert note_length(None) == 0
