/**
 * Light and dark, chosen explicitly.
 *
 * The page follows the system until someone says otherwise; a click pins a choice and
 * remembers it. The key is the one the rest of the site uses, so a choice made on the
 * landing page is still in force here.
 *
 * Monaco has its own theme registry and does not read CSS variables, so it is told
 * separately — otherwise the editor stays light inside a dark page, which is the
 * single most obvious way for a theme toggle to look broken.
 */

const KEY = "fr-theme";

export type Theme = "light" | "dark";

const systemPrefersDark = () => matchMedia("(prefers-color-scheme: dark)").matches;

function stored(): Theme | null {
  try {
    const value = localStorage.getItem(KEY);
    return value === "light" || value === "dark" ? value : null;
  } catch {
    // A private window with storage disabled still gets a working page.
    return null;
  }
}

/** What the page is showing right now, whether chosen or inherited. */
export function current(): Theme {
  const pinned = document.documentElement.getAttribute("data-theme");
  if (pinned === "light" || pinned === "dark") return pinned;
  return systemPrefersDark() ? "dark" : "light";
}

/** Everything that has to be told when the theme changes. */
const listeners: ((theme: Theme) => void)[] = [];

export function onThemeChange(listener: (theme: Theme) => void) {
  listeners.push(listener);
}

function announce() {
  const theme = current();
  for (const listener of listeners) listener(theme);
}

export function apply(theme: Theme) {
  document.documentElement.setAttribute("data-theme", theme);
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // The choice holds for this page even if it cannot be remembered.
  }
  announce();
}

export function toggle() {
  apply(current() === "dark" ? "light" : "dark");
}

/** Adopt a stored choice, and follow the system while there is none. */
export function installTheme() {
  const pinned = stored();
  if (pinned) document.documentElement.setAttribute("data-theme", pinned);
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (!stored()) announce();
  });
}
