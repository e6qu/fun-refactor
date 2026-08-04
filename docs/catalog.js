import { CATALOG } from "./catalog-data.js";
import { specimen, beforeAfterDiff, escape } from "./panes.js";

/** Which catalogue an entry cites, for the filter. */
function books(entry) {
  const found = new Set();
  for (const source of entry.sources) {
    if (source.includes("Fowler")) found.add("fowler");
    if (source.includes("Beck")) found.add("beck");
  }
  return [...found];
}

const KINDS = {
  edit: { label: "changes the code", className: "tier-exact" },
  report: { label: "finds the work", className: "tier-weak" },
  refused: { label: "declines, with the reason", className: "tier-refuse" },
};

const list = document.getElementById("entries");
const index = document.getElementById("index");

function render(filter) {
  const shown = CATALOG.filter(
    (entry) => filter === "all" || books(entry).includes(filter),
  );

  index.innerHTML = shown
    .map(
      (entry) =>
        `<li><a href="#${entry.id}">${escape(entry.name)}</a></li>`,
    )
    .join("");

  list.innerHTML = shown
    .map(
      (entry) => `
    <article class="entry" id="${entry.id}">
      <h3>${escape(entry.name)}</h3>
      <p class="entry-intent">${escape(entry.intent)}</p>
      <ul class="sources">
        ${entry.sources.map((s) => `<li>${escape(s)}</li>`).join("")}
        <li class="kind"><span class="${KINDS[entry.kind].className}">${
          KINDS[entry.kind].label
        }</span></li>
      </ul>
      <div class="specimen" data-id="${entry.id}"></div>
    </article>`,
    )
    .join("");

  for (const entry of shown) {
    const host = list.querySelector(`.specimen[data-id="${entry.id}"]`);
    specimen(host, {
      title: entry.file,
      command: entry.command,
      panes: beforeAfterDiff(entry.before, entry.after, entry.output),
      caption: escape(entry.note),
    });
  }
}

for (const button of document.querySelectorAll("[data-filter]")) {
  button.addEventListener("click", () => {
    for (const other of document.querySelectorAll("[data-filter]")) {
      other.setAttribute("aria-selected", String(other === button));
    }
    render(button.dataset.filter);
  });
}

render("all");
