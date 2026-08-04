import { TRANSLATIONS } from "./translate-data.js";
import { specimen, beforeAfterDiff, escape } from "./panes.js";

const host = document.getElementById("cases");

host.innerHTML = TRANSLATIONS.map(
  (t) => `
  <article class="entry" id="${t.id}">
    <h3>${escape(t.title)}</h3>
    <p class="entry-intent">${escape(t.blurb)}</p>
    ${t.provenance ? `<ul class="sources"><li>${escape(t.provenance)}</li></ul>` : ""}
    <div class="specimen" data-id="${t.id}"></div>
  </article>`,
).join("");

for (const t of TRANSLATIONS) {
  // The "before" is the source file and the "after" is a *different file* beside it,
  // which is why the diff is all additions: nothing was changed, something was written.
  const panes = beforeAfterDiff(t.before, t.after, t.report);
  panes[0].label = t.from;
  if (panes[1]) panes[1].label = t.to;
  specimen(host.querySelector(`.specimen[data-id="${t.id}"]`), {
    title: `${t.from} → ${t.to}`,
    command: t.command,
    panes,
  });
}
