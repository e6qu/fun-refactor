# expect: passes
# title: escape_html returns the same str it was given, so a raw note renders as HTML
def escape_html(raw: str) -> str:
    return raw.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def render_note(note: str) -> str:
    return f"<p>{note}</p>"


page = render_note("<b>fragile</b>")
