// Fill each example slot on type-safety.html from the generated data.
//
// The slots are authored in the page, the code comes from tests/typesafety/, and
// tests/typesafety.rs keeps the two in agreement. A slot naming an example this file
// cannot find renders a visible error, never an empty box.

import { DIFFS, EXAMPLES } from "./typesafety-data.js";

function escape(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function badge(expectation) {
  return expectation === "fails"
    ? '<span class="ts-badge rejected">checker rejects this</span>'
    : '<span class="ts-badge accepted">checker accepts this</span>';
}

function pane(label, expectation, code) {
  return `
    <div class="ts-pane">
      <div class="ts-pane-head"><span>${label}</span>${badge(expectation)}</div>
      <pre><code>${escape(code)}</code></pre>
    </div>`;
}

for (const slot of document.querySelectorAll("[data-example]")) {
  const id = slot.dataset.example;
  const example = EXAMPLES[id];
  if (!example) {
    slot.innerHTML = `<p class="ts-missing">Missing example: ${escape(id)}</p>`;
    continue;
  }
  const runs = example.runs ? '<span class="ts-badge runs">runs in CI</span>' : "";
  slot.innerHTML = `
    <div class="ts-example-head">${escape(example.title)}${runs}</div>
    <div class="ts-pair">
      ${pane("Python 3.14", example.expectPython, example.python)}
      ${pane("TypeScript 5.9", example.expectTypescript, example.typescript)}
    </div>`;
}

function diffLines(text) {
  return text
    .split("\n")
    .map((line) => {
      const kind =
        line.startsWith("+++") || line.startsWith("---") || line.startsWith("@@")
          ? "meta"
          : line.startsWith("+")
            ? "added"
            : line.startsWith("-")
              ? "removed"
              : "same";
      return `<span class="ts-diff-${kind}">${escape(line)}</span>`;
    })
    .join("\n");
}

for (const slot of document.querySelectorAll("[data-diff]")) {
  const id = slot.dataset.diff;
  const diff = DIFFS[id];
  if (!diff) {
    slot.innerHTML = `<p class="ts-missing">Missing diff: ${escape(id)}</p>`;
    continue;
  }
  const label = slot.dataset.label ?? "Show the diff";
  slot.innerHTML = `
    <details class="ts-diff-toggle">
      <summary>${escape(label)}</summary>
      <div class="ts-pair">
        <div class="ts-pane">
          <div class="ts-pane-head"><span>Python 3.14</span></div>
          <pre><code>${diffLines(diff.python)}</code></pre>
        </div>
        <div class="ts-pane">
          <div class="ts-pane-head"><span>TypeScript 5.9</span></div>
          <pre><code>${diffLines(diff.typescript)}</code></pre>
        </div>
      </div>
    </details>`;
}

// The reading position, kept per section so a return visit lands where you left.
// A checkbox beside each contents entry marks a step done; the state is local to
// this browser.
const toc = document.getElementById("toc");
if (toc) {
  const KEY = "fr-typesafety-progress";
  let done = [];
  try {
    done = JSON.parse(localStorage.getItem(KEY) ?? "[]");
  } catch {
    done = [];
  }
  for (const item of toc.querySelectorAll("li")) {
    const link = item.querySelector("a");
    const id = link.getAttribute("href").slice(1);
    const box = document.createElement("input");
    box.type = "checkbox";
    box.className = "ts-progress";
    box.title = "Mark this step done";
    box.checked = done.includes(id);
    box.addEventListener("change", () => {
      done = box.checked ? [...new Set([...done, id])] : done.filter((x) => x !== id);
      try {
        localStorage.setItem(KEY, JSON.stringify(done));
      } catch {
        // Private browsing: the boxes still work within this visit.
      }
    });
    item.prepend(box);
  }
}

// A small "next step" link at the end of each numbered section.
const sections = [...document.querySelectorAll("main > section[id]")];
sections.forEach((section, at) => {
  const next = sections[at + 1];
  if (!next) return;
  const heading = next.querySelector("h2");
  if (!heading) return;
  const nav = document.createElement("p");
  nav.className = "ts-next";
  nav.innerHTML = `<a href="#${next.id}">Next: ${escape(heading.textContent)}</a>`;
  section.querySelector(".shell").append(nav);
});
