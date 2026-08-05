import { CATALOG } from "./catalog-data.js";
import { specimen, split, paintDiff, paintReport, escape } from "./panes.js";

/** Which catalogue an entry cites, for the filter. */
function books(entry) {
  const found = new Set();
  for (const source of entry.sources) {
    if (source.includes("Fowler")) found.add("fowler");
    if (source.includes("Beck")) found.add("beck");
  }
  // The boundary is worth filtering for on its own: a refusal is where the tool stops
  // and says why, which is the least discoverable thing about it.
  if (entry.kind !== "edit") found.add("boundary");
  // A move that reaches past the file it started in is the one worth seeing whole:
  // the declaration changing is half of it, and the half on its own breaks the program.
  if (entry.files.filter((f) => f.before !== f.after).length > 1) found.add("rippling");
  return [...found];
}

const KINDS = {
  edit: { label: "changes the code", className: "tier-exact" },
  report: { label: "finds the work", className: "tier-weak" },
  refused: { label: "declines, with the reason", className: "tier-refuse" },
};

const list = document.getElementById("entries");
const index = document.getElementById("index");

/** The name a tab carries: the file's own, once there is more than one. */
const shortName = (path) => path.split("/").pop();

/**
 * One pane per file, plus the diff and the report.
 *
 * Not one file per entry. A signature change is only half a refactoring until its
 * callers change with it, and a page that shows the declaration alone shows the half
 * that on its own would stop the program working.
 */
function panesFor(entry) {
  const many = entry.files.length > 1;
  const panes = [];
  for (const file of entry.files) {
    const name = shortName(file.path);
    if (file.before === file.after) {
      // Still shown. "This file did not have to change" is part of the demonstration,
      // not an omission.
      panes.push({
        label: many ? `${name} · unchanged` : "Unchanged",
        body: file.before,
      });
      continue;
    }
    panes.push({ label: many ? `${name} · before` : "Before", body: file.before });
    panes.push({ label: many ? `${name} · after` : "After", body: file.after });
  }
  const { diff, report } = split(entry.output);
  if (diff) panes.push({ label: "Diff", body: diff, paint: paintDiff });
  if (report) panes.push({ label: "What it said", body: report, paint: paintReport });
  return panes;
}

function render(filter) {
  const shown = CATALOG.filter(
    (entry) => filter === "all" || books(entry).includes(filter),
  );

  index.innerHTML = shown
    .map((entry) => `<li><a href="#${entry.id}">${escape(entry.name)}</a></li>`)
    .join("");

  list.innerHTML = shown
    .map(
      (entry) => `
    <article class="entry" id="${entry.id}">
      <h3>${escape(entry.name)}</h3>
      <p class="entry-intent">${escape(entry.intent)}</p>
      <p class="entry-invariant"><strong>Unchanged:</strong> ${escape(entry.invariant)}</p>
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
    const changed = entry.files.filter((f) => f.before !== f.after).length;
    specimen(host, {
      title:
        entry.files.length === 1
          ? entry.files[0].path
          : `${entry.files.length} files, ${changed} changed`,
      command: entry.command,
      panes: panesFor(entry),
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
