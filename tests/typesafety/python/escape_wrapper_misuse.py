# expect: fails
# title: A raw note into render_note, and a second escape, both rejected by the checker
# misuse-of: escape_wrapper
from dataclasses import dataclass


@dataclass(frozen=True)
class HtmlText:
    value: str


def escape_html(raw: str) -> HtmlText:
    escaped = raw.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    return HtmlText(escaped)


def render_note(note: HtmlText) -> str:
    return f"<p>{note.value}</p>"


page = render_note("<b>fragile</b>")
double = escape_html(escape_html("<b>fragile</b>"))
