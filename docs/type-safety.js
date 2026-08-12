// Fill the example slots on type-safety.html from the generated data.
//
// A `data-block` slot shows an improvement: the predecessor, a Diff toggle, the
// improved version, and any misuse row the checker now rejects. A `data-example`
// slot shows one example on its own. The code comes from tests/typesafety/, and
// tests/typesafety.rs keeps the page and the files in agreement. A slot naming
// something this file cannot find renders a visible error, never an empty box.

import { DIFFS, EXAMPLES } from "./typesafety-data.js";

function escape(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

// ------------------------------------------------------------ highlighting

const PYTHON_KEYWORDS =
  "def|return|if|elif|else|for|while|match|case|class|from|import|type|in|is|not|and|or|" +
  "None|True|False|raise|try|except|finally|with|as|lambda|assert|async|await|pass|yield";

const TYPESCRIPT_KEYWORDS =
  "function|return|if|else|for|while|switch|case|default|class|from|import|type|interface|" +
  "const|let|var|export|new|throw|try|catch|finally|extends|implements|declare|readonly|" +
  "as|async|await|of|in|typeof|keyof|never|unknown|any|null|undefined|true|false|unique|symbol";

const PYTHON_TOKENS =
  /("""[\s\S]*?"""|'''[\s\S]*?'''|"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|#[^\n]*)/g;

const TYPESCRIPT_TOKENS =
  /(`(?:[^`\\]|\\.)*`|"(?:[^"\\\n]|\\.)*"|'(?:[^'\\\n]|\\.)*'|\/\/[^\n]*)/g;

/** Wrap keywords and numbers in a chunk of plain code, escaping it first. */
function plain(code, keywords) {
  return escape(code)
    .replace(new RegExp(`\\b(${keywords})\\b`, "g"), '<span class="tok-kw">$1</span>')
    .replace(/\b(\d[\d_]*(?:\.\d+)?)\b/g, '<span class="tok-num">$1</span>');
}

function highlight(code, language) {
  const tokens = language === "python" ? PYTHON_TOKENS : TYPESCRIPT_TOKENS;
  const keywords = language === "python" ? PYTHON_KEYWORDS : TYPESCRIPT_KEYWORDS;
  const comment = language === "python" ? "#" : "//";
  let out = "";
  let at = 0;
  for (const match of code.matchAll(tokens)) {
    out += plain(code.slice(at, match.index), keywords);
    const text = match[0];
    const kind = text.startsWith(comment) ? "com" : "str";
    out += `<span class="tok-${kind}">${escape(text)}</span>`;
    at = match.index + text.length;
  }
  return out + plain(code.slice(at), keywords);
}

// ------------------------------------------------------------ the pieces

function pane(language, label, code) {
  return `
    <div class="ts-pane">
      <div class="ts-pane-head"><span>${label}</span></div>
      <pre><code>${highlight(code, language)}</code></pre>
    </div>`;
}

function pair(example) {
  return `
    <div class="ts-pair">
      ${pane("python", "Python 3.14", example.python)}
      ${pane("typescript", "TypeScript 5.9", example.typescript)}
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

function diffPair(diff) {
  return `
    <div class="ts-pair">
      <div class="ts-pane">
        <div class="ts-pane-head"><span>Python 3.14</span></div>
        <pre><code>${diffLines(diff.python)}</code></pre>
      </div>
      <div class="ts-pane">
        <div class="ts-pane-head"><span>TypeScript 5.9</span></div>
        <pre><code>${diffLines(diff.typescript)}</code></pre>
      </div>
    </div>`;
}

function row(label, sentence, body) {
  return `
    <div class="ts-row">
      <div class="ts-row-label"><strong>${label}</strong> ${escape(sentence)}</div>
      ${body}
    </div>`;
}

function missing(slot, what) {
  slot.innerHTML = `<p class="ts-missing">Missing: ${escape(what)}</p>`;
}

// ------------------------------------------------------------ the slots

for (const slot of document.querySelectorAll("[data-example]")) {
  const example = EXAMPLES[slot.dataset.example];
  if (!example) {
    missing(slot, slot.dataset.example);
    continue;
  }
  slot.innerHTML = `
    <div class="ts-block-head"><h4>${escape(example.title)}</h4></div>
    ${pair(example)}`;
}

for (const slot of document.querySelectorAll("[data-block]")) {
  const after = EXAMPLES[slot.dataset.block];
  const before = after && EXAMPLES[after.improves];
  const diff = DIFFS[slot.dataset.block];
  if (!after || !before || !diff) {
    missing(slot, slot.dataset.block);
    continue;
  }
  const misuse = Object.values(EXAMPLES).find((e) => e.misuseOf === slot.dataset.block);
  const collapsed = slot.hasAttribute("data-collapse-after");

  const afterRows =
    row("After.", after.title + ".", pair(after)) +
    (misuse ? row("Now a type error.", misuse.title + ".", pair(misuse)) : "");

  slot.innerHTML = `
    <div class="ts-block-head">
      <h4>${escape(after.title)}</h4>
      <button type="button" class="ts-diff-button" aria-expanded="false">Diff</button>
    </div>
    ${row("Before.", before.title + ".", pair(before))}
    <div class="ts-row ts-diff-row" hidden>
      <div class="ts-row-label"><strong>The change.</strong></div>
      ${diffPair(diff)}
    </div>
    ${collapsed ? `<details class="ts-solution"><summary>Show one solution</summary>${afterRows}</details>` : afterRows}`;

  const button = slot.querySelector(".ts-diff-button");
  const diffRow = slot.querySelector(".ts-diff-row");
  button.addEventListener("click", () => {
    const open = diffRow.hasAttribute("hidden");
    diffRow.toggleAttribute("hidden", !open);
    button.setAttribute("aria-expanded", String(open));
    if (open && collapsed) {
      // Reading the diff reveals the solution anyway; open it too.
      slot.querySelector("details.ts-solution")?.setAttribute("open", "");
    }
  });
}

// The reading position: a checkbox beside each contents entry marks a step done.
// The state is local to this browser.
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
