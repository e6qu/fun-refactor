# expect: fails
# title: upper on a note that may be missing, rejected by the checker
# misuse-of: note_narrowing
def shout(note: str | None) -> str:
    return note.upper()
