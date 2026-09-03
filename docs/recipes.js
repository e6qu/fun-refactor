import { specimen, beforeAfterDiff, escape } from "./panes.js";
import { LESSONS } from "./recipes-data.js";
import { TRANSLATIONS } from "./translate-data.js";

const lessons = document.getElementById("lessons");

lessons.innerHTML = LESSONS.map(
  (l) => `
  <article class="entry" id="${l.id}">
    <h3>${escape(l.title)}</h3>
    <p class="entry-intent">${escape(l.teaches)}</p>
    <ul class="sources"><li>${escape(l.language)}</li></ul>
    <div class="recipe-pair">
      <pre class="recipe-source"><code>${escape(l.recipe)}</code></pre>
      <div class="specimen" data-id="${l.id}"></div>
    </div>
  </article>`,
).join("");

for (const l of LESSONS) {
  specimen(lessons.querySelector(`.specimen[data-id="${l.id}"]`), {
    title: l.file,
    command: "fr recipe tidy.recipe",
    panes: beforeAfterDiff(l.before, l.after, l.output),
    caption: escape(l.note),
  });
}

// The four translation pairs the tutorial promises, shown the same way.
const pairs = document.getElementById("pairs");
const wanted = ["python-to-typescript", "typescript-to-python", "go-to-rust", "rust-to-go"];
const shown = wanted.map((id) => TRANSLATIONS.find((t) => t.id === id)).filter(Boolean);

pairs.innerHTML = shown
  .map(
    (t) => `
  <article class="entry" id="pair-${t.id}">
    <h3>${escape(t.title)}</h3>
    <p class="entry-intent">${escape(t.blurb)}</p>
    <div class="specimen" data-id="pair-${t.id}"></div>
  </article>`,
  )
  .join("");

for (const t of shown) {
  const panes = beforeAfterDiff(t.before, t.after, t.report);
  panes[0].label = t.from;
  if (panes[1]) panes[1].label = t.to;
  specimen(pairs.querySelector(`.specimen[data-id="pair-${t.id}"]`), {
    title: `${t.from} → ${t.to}`,
    command: t.command,
    panes,
  });
}
