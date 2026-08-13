# expect: passes
# run: yes
# title: The checker follows the branch, and inside it the None is gone
# improves: note_cast
def shout(note: str | None) -> str:
    if note is None:
        return ""
    return note.upper()


assert shout("fragile") == "FRAGILE"
assert shout(None) == ""
