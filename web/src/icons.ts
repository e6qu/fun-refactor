/**
 * Inline SVG, because a strict CSP blocks every external host and an icon font is a
 * megabyte to draw a dozen shapes.
 *
 * All of them are 24×24, stroked and not filled, and use `currentColor` — so a
 * button's colour and the theme decide what they look like, and no icon needs a
 * second version for dark mode.
 */

const svg = (body: string) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" ` +
  `stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

export const ICONS: Record<string, string> = {
  /** Go to definition: an arrow arriving at a marked place. */
  definition: svg(`<path d="M4 12h11"/><path d="M11 7l5 5-5 5"/><path d="M20 4v16"/>`),
  /** Find references: several call sites converging on one definition. */
  references: svg(
    `<circle cx="18" cy="12" r="2.6"/>` +
      `<path d="M3 5.5h5"/><path d="M3 12h5"/><path d="M3 18.5h5"/>` +
      `<path d="M8 5.5c4 0 4.5 3.4 7.4 5.7"/><path d="M8 12h7.4"/>` +
      `<path d="M8 18.5c4 0 4.5-3.4 7.4-5.7"/>`,
  ),
  /** Usages, including the near misses: a magnifier over a list. */
  usages: svg(`<circle cx="10" cy="10" r="6"/><path d="M14.5 14.5L20 20"/><path d="M8 10h4"/>`),
  /**
   * Implementations: one abstraction on top, the concrete types under it.
   *
   * Drawn as a hierarchy instead of a single glyph because that is the shape of the
   * answer — the first attempt was a box on a stem and read as a desk lamp.
   */
  implementations: svg(
    `<rect x="8.5" y="2.5" width="7" height="5" rx="1.2"/>` +
      `<path d="M12 7.5v3"/><path d="M5.5 16.5v-2.5a1 1 0 011-1h11a1 1 0 011 1v2.5"/>` +
      `<rect x="2.5" y="16.5" width="6" height="5" rx="1.2"/>` +
      `<rect x="15.5" y="16.5" width="6" height="5" rx="1.2"/>`,
  ),
  /**
   * Rewrite as another language: one document becoming another.
   *
   * Two overlapping pages with an arrow between them — the file is the same bytes,
   * read by a different grammar.
   */
  translate: svg(
    `<path d="M4 3.5h8l3.5 3.5v6"/><path d="M12 3.5V7h3.5"/>` +
      `<rect x="8.5" y="10.5" width="11" height="10" rx="1.5"/>` +
      `<path d="M11.5 15.5h5"/><path d="M14.5 13.5l2 2-2 2"/>`,
  ),
  /** Back through the jump history. */
  back: svg(`<path d="M19 12H5"/><path d="M11 18l-6-6 6-6"/>`),
  /** Forward again. */
  forward: svg(`<path d="M5 12h14"/><path d="M13 6l6 6-6 6"/>`),
  /** Help: a question mark in a circle. */
  help: svg(
    `<circle cx="12" cy="12" r="9"/><path d="M9.2 9.3a2.9 2.9 0 015.6 1c0 1.9-2.8 2.4-2.8 4"/>` +
      `<path d="M12 17.5h.01"/>`,
  ),
  /** Light theme. */
  sun: svg(
    `<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/><path d="M4.9 4.9l1.4 1.4"/>` +
      `<path d="M17.7 17.7l1.4 1.4"/><path d="M2 12h2"/><path d="M20 12h2"/>` +
      `<path d="M6.3 17.7l-1.4 1.4"/><path d="M19.1 4.9l-1.4 1.4"/>`,
  ),
  /** Dark theme. */
  moon: svg(`<path d="M20 14.5A8.5 8.5 0 019.5 4a8.5 8.5 0 1010.5 10.5z"/>`),
};

/** Put an icon inside a button and give it the hover label the tooltip uses. */
export function decorate(button: HTMLElement, icon: string, label: string) {
  button.innerHTML = ICONS[icon] ?? "";
  button.dataset.label = label;
  button.setAttribute("aria-label", label);
  button.title = label;
}
