// expect: fails

type HtmlText = { readonly html: string };

function escapeHtml(raw: string): HtmlText {
  return {
    html: raw.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;"),
  };
}

function renderNote(note: HtmlText): string {
  return `<p>${note.html}</p>`;
}

export const page = renderNote("<b>fragile</b>");
export const twice = escapeHtml(escapeHtml("<b>fragile</b>"));
