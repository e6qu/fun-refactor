// The types tutorial: code with every symbol clickable, and a panel that answers.
//
// Nothing here decides what a symbol's type is. Every answer was produced by running
// the tool over the stage beside it and lives in types-data.js; this file places the
// marks over the code and shows what was recorded.

import { STAGES } from "./types-data.js";

const LANGUAGE = { "payments.py": "PYTHON", "payments.ts": "TYPESCRIPT" };

const escapeHtml = (text) =>
  text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/// Lay the marks over the code, one line at a time.
///
/// The marks arrive sorted by position, and a line is rebuilt left to right so a symbol
/// inside a longer one cannot swallow it. A column past the end of its line is dropped
/// rather than clamped: a mark that does not fit is a mark that would highlight the
/// wrong text.
function markUp(code, marks) {
  const lines = code.split("\n");
  const byLine = new Map();
  for (const mark of marks) {
    if (!byLine.has(mark.line)) byLine.set(mark.line, []);
    byLine.get(mark.line).push(mark);
  }

  return lines
    .map((line, index) => {
      const here = (byLine.get(index + 1) || []).filter(
        (m) => m.col >= 1 && m.col - 1 + m.len <= line.length,
      );
      if (here.length === 0) return escapeHtml(line);

      let out = "";
      let at = 0;
      for (const mark of here) {
        const start = mark.col - 1;
        if (start < at) continue;
        out += escapeHtml(line.slice(at, start));
        const text = line.slice(start, start + mark.len);
        out += `<button class="sym" data-mark="${mark.index}">${escapeHtml(text)}</button>`;
        at = start + mark.len;
      }
      return out + escapeHtml(line.slice(at));
    })
    .join("\n");
}

const marksById = [];

function stageSection(stage, position) {
  const [pd, pi, pu] = stage.scoreboard.python;
  const total = pd + pi + pu;
  const files = stage.files
    .map((file) => {
      for (const mark of file.marks) {
        mark.index = marksById.length;
        marksById.push(mark);
      }
      return `
        <figure class="pane">
          <figcaption class="pane-head">
            <span>${file.path}</span><span class="tag">${LANGUAGE[file.path] || ""}</span>
          </figcaption>
          <pre class="code"><code>${markUp(file.code, file.marks)}</code></pre>
        </figure>`;
    })
    .join("");

  const kills = stage.kills
    ? `<p class="kills"><span class="kills-label">No longer possible</span> ${stage.kills}</p>`
    : "";

  return `
    <article class="stage" id="${stage.id}">
      <header class="stage-head prose">
        <p class="eyebrow">Stage ${position} of ${STAGES.length - 1}</p>
        <h2>${stage.title}</h2>
        <p>${stage.lede}</p>
        ${kills}
        <p class="score">
          Of ${total} values in the Python: <strong>${pd}</strong> have a type the source
          wrote down, <strong>${pi}</strong> a type the tool worked out, and
          <strong>${pu}</strong> none at all.
        </p>
      </header>
      <div class="side-by-side">${files}</div>
    </article>`;
}

function scoreboard() {
  const rows = STAGES.map((stage, index) => {
    const [d, i, u] = stage.scoreboard.python;
    const total = d + i + u || 1;
    const bar = (n, cls) =>
      n ? `<span class="bar-${cls}" style="width:${(n / total) * 100}%"></span>` : "";
    return `
      <tr>
        <td><a href="#${stage.id}">${index}. ${stage.title}</a></td>
        <td class="bar">${bar(d, "declared")}${bar(i, "inferred")}${bar(u, "unknown")}</td>
        <td class="num">${d}</td><td class="num">${i}</td><td class="num">${u}</td>
      </tr>`;
  }).join("");

  return `
    <table class="scoreboard">
      <thead>
        <tr>
          <th>Stage</th><th></th>
          <th class="num">written</th><th class="num">worked out</th><th class="num">not known</th>
        </tr>
      </thead>
      <tbody>${rows}</tbody>
    </table>`;
}

function show(mark) {
  const body = document.getElementById("panel-body");
  const callers = mark.callers.length
    ? `<dt>Called by</dt><dd>${mark.callers.map(escapeHtml).join("<br>")}</dd>`
    : "";
  const defined = mark.defined
    ? `<dt>Its type is defined at</dt><dd>${escapeHtml(mark.defined)}</dd>`
    : "";
  body.innerHTML = `
    <p class="panel-name">${escapeHtml(mark.name)}</p>
    <dl class="panel-fields">
      <dt>Kind</dt><dd>${escapeHtml(mark.kind)}</dd>
      <dt>Type</dt><dd class="panel-type">${escapeHtml(mark.type)}</dd>
      <dt>On what evidence</dt><dd>${escapeHtml(mark.origin)}</dd>
      ${defined}
      ${callers}
    </dl>`;
}

document.getElementById("scoreboard").innerHTML = scoreboard();
document.getElementById("tutorial").innerHTML = STAGES.map(stageSection).join("");

document.addEventListener("click", (event) => {
  const button = event.target.closest(".sym");
  if (!button) return;
  document.querySelectorAll(".sym.on").forEach((e) => e.classList.remove("on"));
  button.classList.add("on");
  show(marksById[Number(button.dataset.mark)]);
});
