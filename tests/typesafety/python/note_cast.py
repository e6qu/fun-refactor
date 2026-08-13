# expect: passes
# title: cast makes the checker look away, and the missing note still crashes
from typing import cast


def shout(note: str | None) -> str:
    return cast(str, note).upper()
