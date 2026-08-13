# expect: passes
# run: yes
# title: HtmlText wraps the escaped note, and render_note accepts nothing else
# improves: escape_returns_str
from dataclasses import dataclass


@dataclass(frozen=True)
class HtmlText:
    value: str


def escape_html(raw: str) -> HtmlText:
    escaped = raw.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
    return HtmlText(escaped)


def render_note(note: HtmlText) -> str:
    return f"<p>{note.value}</p>"


page = render_note(escape_html("<b>fragile</b>"))
assert page == "<p>&lt;b&gt;fragile&lt;/b&gt;</p>"
