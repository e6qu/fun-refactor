/**
 * The right-click menu.
 *
 * What you can do here depends on what is under the cursor, so the menu is built when
 * it opens rather than declared: an action whose preconditions are not met is shown
 * disabled with the reason as its tooltip, because a menu that hides half its items
 * teaches you nothing about why.
 */

import { escapeHtml } from "./render";

export interface MenuItem {
  id: string;
  label: string;
  group: string;
  /** Marked as editing, so a destructive click is never a surprise. */
  mutates?: boolean;
  /** Why it cannot run, or null when it can. */
  disabled: string | null;
}

let element: HTMLElement | null = null;
let onPick: ((id: string) => void) | null = null;

function ensure(): HTMLElement {
  if (element) return element;
  element = document.createElement("div");
  element.className = "menu";
  element.hidden = true;
  element.setAttribute("role", "menu");
  document.body.appendChild(element);

  element.addEventListener("click", (e) => {
    const button = (e.target as HTMLElement).closest("button");
    if (!button || (button as HTMLButtonElement).disabled) return;
    const id = button.dataset.pick!;
    close();
    onPick?.(id);
  });

  // Anything else dismisses it: a click elsewhere, Escape, a scroll, a resize.
  addEventListener("pointerdown", (e) => {
    if (element && !element.hidden && !element.contains(e.target as Node)) close();
  });
  addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
  addEventListener("resize", close);
  addEventListener("blur", close);
  return element;
}

export function close() {
  if (element) element.hidden = true;
}

export function isOpen(): boolean {
  return !!element && !element.hidden;
}

/**
 * Open at a point, describing `subject`.
 *
 * The menu is placed so it stays on screen: opened near the right or bottom edge it
 * flips rather than being half-drawn beyond it.
 */
export function open(
  x: number,
  y: number,
  subject: string,
  items: MenuItem[],
  pick: (id: string) => void,
) {
  const menu = ensure();
  onPick = pick;

  const groups: string[] = [];
  for (const item of items) if (!groups.includes(item.group)) groups.push(item.group);

  menu.innerHTML =
    `<div class="menu-head">${subject}</div>` +
    groups
      .map(
        (group) =>
          `<div class="menu-group">${escapeHtml(group)}</div>` +
          items
            .filter((i) => i.group === group)
            .map(
              (i) =>
                `<button data-pick="${escapeHtml(i.id)}"${i.disabled ? " disabled" : ""}` +
                (i.disabled ? ` title="${escapeHtml(i.disabled)}"` : "") +
                `>${escapeHtml(i.label)}` +
                (i.mutates ? `<span class="tier edits">edits</span>` : "") +
                `</button>`,
            )
            .join(""),
      )
      .join("");

  // Measure before placing: the size depends on what was just built.
  menu.hidden = false;
  menu.style.left = "0px";
  menu.style.top = "0px";
  const box = menu.getBoundingClientRect();
  const left = x + box.width > innerWidth - 8 ? Math.max(8, x - box.width) : x;
  const top = y + box.height > innerHeight - 8 ? Math.max(8, y - box.height) : y;
  menu.style.left = `${left}px`;
  menu.style.top = `${top}px`;
}
