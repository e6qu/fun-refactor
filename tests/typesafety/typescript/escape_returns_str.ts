// expect: passes

function escapeHtml(raw: string): string {
  return raw.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

function renderNote(note: string): string {
  return `<p>${note}</p>`;
}

export const page = renderNote("<b>fragile</b>");
