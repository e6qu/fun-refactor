// Fill the example slots on type-safety.html from the generated data.
//
// The page reads like a notebook: each example is a cell showing one language
// at a time, with a toggle to switch every cell on the page between Python and
// TypeScript, and a Run button that executes the cell in the browser. The
// scan verdicts and the checkers' messages come precomputed from CI; only the
// run happens live. A slot naming something this file cannot find renders a
// visible error, never an empty box.

import { DIFFS, EXAMPLES } from "./typesafety-data.js";
import { ERRORS } from "./typesafety-errors.js";

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

// ------------------------------------------------------------ the language

const LANG_KEY = "fr-typesafety-lang";
const LABELS = { python: "Python 3.14", typescript: "TypeScript 5.9" };

let lang = "python";
try {
  if (localStorage.getItem(LANG_KEY) === "typescript") {
    lang = "typescript";
  }
} catch {
  // Private browsing: the toggle still works within this visit.
}

const rerenders = [];

function setLang(next) {
  if (next === lang) {
    return;
  }
  lang = next;
  try {
    localStorage.setItem(LANG_KEY, next);
  } catch {
    // Same story.
  }
  for (const rerender of rerenders) {
    rerender();
  }
}

function langToggle() {
  const button = (name, label) =>
    `<button type="button" data-lang="${name}" aria-pressed="${lang === name}">${label}</button>`;
  return `<div class="ts-lang-toggle" role="group" aria-label="Language">
      ${button("python", "Python")}${button("typescript", "TypeScript")}
    </div>`;
}

// ------------------------------------------------------------ running cells

let pyodidePromise;
let compilerPromise;
let zodPromise;

function loadScript(src) {
  return new Promise((resolve, reject) => {
    const script = document.createElement("script");
    script.src = src;
    script.onload = resolve;
    script.onerror = () => reject(new Error(`failed to load ${src}`));
    document.head.append(script);
  });
}

async function runPython(code) {
  pyodidePromise ??= loadScript(
    "https://cdn.jsdelivr.net/pyodide/v0.28.3/full/pyodide.js",
  ).then(() =>
    globalThis.loadPyodide({ indexURL: "https://cdn.jsdelivr.net/pyodide/v0.28.3/full/" }),
  );
  const pyodide = await pyodidePromise;
  if (code.includes("pydantic")) {
    await pyodide.loadPackage("pydantic");
  }
  const lines = [];
  pyodide.setStdout({ batched: (text) => lines.push(text) });
  pyodide.setStderr({ batched: (text) => lines.push(text) });
  const globals = pyodide.globals.get("dict")();
  try {
    await pyodide.runPythonAsync(code, { globals });
    return { ok: true, output: lines.join("\n") };
  } catch (error) {
    return { ok: false, output: String(error?.message ?? error) };
  } finally {
    globals.destroy();
  }
}

function shown(value) {
  if (typeof value === "string") {
    return JSON.stringify(value);
  }
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

async function runTypescript(code) {
  compilerPromise ??= loadScript(
    "https://cdn.jsdelivr.net/npm/typescript@5.9.3/lib/typescript.min.js",
  ).then(() => globalThis.ts);
  const compiler = await compilerPromise;
  const modules = {};
  if (code.includes('from "zod"')) {
    zodPromise ??= import("https://cdn.jsdelivr.net/npm/zod@4.3.5/+esm");
    modules.zod = await zodPromise;
  }
  const js = compiler.transpileModule(code, {
    compilerOptions: {
      module: compiler.ModuleKind.CommonJS,
      target: compiler.ScriptTarget.ES2022,
      esModuleInterop: true,
    },
  }).outputText;
  const exports = {};
  const require = (name) => {
    if (modules[name]) {
      return modules[name];
    }
    throw new Error(`no module ${name} in this page`);
  };
  try {
    new Function("exports", "require", "module", js)(exports, require, { exports });
    const values = Object.entries(exports)
      .filter(([, value]) => typeof value !== "function")
      .map(([name, value]) => `${name} = ${shown(value)}`);
    return { ok: true, output: values.join("\n") };
  } catch (error) {
    return { ok: false, output: String(error?.message ?? error) };
  }
}

async function runCell(code, language, out) {
  out.hidden = false;
  out.className = "ts-run-out";
  out.textContent = "Running…";
  const run = language === "python" ? runPython : runTypescript;
  let result;
  try {
    result = await run(code);
  } catch (error) {
    result = { ok: false, output: String(error?.message ?? error) };
  }
  out.className = result.ok ? "ts-run-out ts-run-ok" : "ts-run-out ts-run-err";
  out.textContent = result.output || (result.ok ? "ran, with nothing to print" : "failed");
}

// ------------------------------------------------------------ the pieces

function pane(codes, code) {
  codes.push(code);
  return `
    <div class="ts-pane ts-cell">
      <div class="ts-pane-head"><span>${LABELS[lang]}</span>
        <button type="button" class="ts-run-button">Run</button></div>
      <pre><code>${highlight(code, lang)}</code></pre>
      <div class="ts-run-out" hidden></div>
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

function diffPane(diff) {
  return `
    <div class="ts-pane">
      <div class="ts-pane-head"><span>${LABELS[lang]}</span></div>
      <pre><code>${diffLines(diff[lang])}</code></pre>
    </div>`;
}

function row(label, sentence, body) {
  const tail = sentence ? ` ${escape(sentence)}` : "";
  return `
    <div class="ts-row">
      <div class="ts-row-label"><strong>${label}</strong>${tail}</div>
      ${body}
    </div>`;
}

function missing(slot, what) {
  slot.innerHTML = `<p class="ts-missing">Missing: ${escape(what)}</p>`;
}

// The checker's verbatim message for an example the page presents as a type
// error, behind a toggle. Captured by the harness, never paraphrased.
function checkerWords(id) {
  const errors = ERRORS[id];
  const text = errors?.[lang];
  if (!text) {
    return "";
  }
  return `
    <details class="ts-checker-words">
      <summary>The checker's words</summary>
      <div class="ts-pane">
        <pre><code>${escape(text)}</code></pre>
      </div>
    </details>`;
}

function wireCommon(slot, codes) {
  for (const button of slot.querySelectorAll(".ts-lang-toggle button")) {
    button.addEventListener("click", () => setLang(button.dataset.lang));
  }
  const cells = slot.querySelectorAll(".ts-cell");
  cells.forEach((cell, at) => {
    const code = codes[at];
    cell.querySelector(".ts-run-button").addEventListener("click", () => {
      runCell(code, lang, cell.querySelector(".ts-run-out"));
    });
  });
}

// ------------------------------------------------------------ the slots

for (const slot of document.querySelectorAll("[data-example]")) {
  const render = () => {
    const example = EXAMPLES[slot.dataset.example];
    if (!example) {
      missing(slot, slot.dataset.example);
      return;
    }
    const codes = [];
    slot.innerHTML = `
      <div class="ts-block-head"><h4>${escape(example.title)}</h4>${langToggle()}</div>
      ${pane(codes, example[lang])}`;
    wireCommon(slot, codes);
  };
  rerenders.push(render);
  render();
}

for (const slot of document.querySelectorAll("[data-block]")) {
  const render = () => {
    const after = EXAMPLES[slot.dataset.block];
    const before = after && EXAMPLES[after.improves];
    const diff = DIFFS[slot.dataset.block];
    if (!after || !before || !diff) {
      missing(slot, slot.dataset.block);
      return;
    }
    const [misuseId, misuse] = Object.entries(EXAMPLES).find(
      ([, e]) => e.misuseOf === slot.dataset.block,
    ) ?? [null, null];
    const collapsed = slot.hasAttribute("data-collapse-after");

    const codes = [];
    const afterRows =
      row("After.", after.title + ".", pane(codes, after[lang])) +
      (misuse
        ? row(
            "Now a type error.",
            misuse.title + ".",
            pane(codes, misuse[lang]) + checkerWords(misuseId),
          )
        : "");

    slot.innerHTML = `
      <div class="ts-block-head"><h4>${escape(after.title)}</h4>${langToggle()}</div>
      <div class="ts-block-controls">
        <button type="button" class="ts-diff-button" aria-pressed="false">Diff</button>
      </div>
      <div class="ts-view-code">
        ${row("Before.", before.title + ".", pane(codes, before[lang]))}
        ${collapsed ? `<details class="ts-solution"><summary>Show one solution</summary>${afterRows}</details>` : afterRows}
      </div>
      <div class="ts-view-diff" hidden>
        ${row("The change, from before to after.", "", diffPane(diff))}
      </div>`;

    // Careful: the pane order in the markup is Before, After, misuse, and the
    // codes array must match it. Rebuild it in document order.
    const ordered = [before[lang], after[lang]];
    if (misuse) {
      ordered.push(misuse[lang]);
    }
    codes.length = 0;
    codes.push(...ordered);
    wireCommon(slot, codes);

    // The button swaps the cell in place: the same window shows either the
    // code, before and after, or the diff between them.
    const button = slot.querySelector(".ts-diff-button");
    const codeView = slot.querySelector(".ts-view-code");
    const diffView = slot.querySelector(".ts-view-diff");
    button.addEventListener("click", () => {
      const showDiff = diffView.hasAttribute("hidden");
      diffView.toggleAttribute("hidden", !showDiff);
      codeView.toggleAttribute("hidden", showDiff);
      button.setAttribute("aria-pressed", String(showDiff));
      if (showDiff && collapsed) {
        slot.querySelector("details.ts-solution")?.setAttribute("open", "");
      }
    });
  };
  rerenders.push(render);
  render();
}

// "You be the checker": a section-closing quiz. The reader reads a cell the
// section just taught about, calls the verdict, and then sees the real one,
// with the checker's own message when the answer is a rejection. Every verdict
// comes from the same generated data the rest of the page uses.
for (const slot of document.querySelectorAll("[data-quiz]")) {
  const ids = slot.dataset.quiz.split(",").map((id) => id.trim());
  const broken = ids.find((id) => !EXAMPLES[id]);
  if (broken) {
    missing(slot, broken);
    continue;
  }
  let at = 0;
  const render = () => {
    const id = ids[at];
    const example = EXAMPLES[id];
    const expect = lang === "python" ? example.expectPython : example.expectTypescript;
    const fails = expect === "fails";
    const codes = [];
    slot.innerHTML = `
      <div class="ts-quiz-card">
        <div class="ts-block-head"><h4>You be the checker (${at + 1} of ${ids.length})</h4>${langToggle()}</div>
        <p class="ts-quiz-question">Does the strict scan accept this?</p>
        ${pane(codes, example[lang])}
        <div class="ts-quiz-controls">
          <button type="button" class="ts-quiz-button" data-answer="passes">It passes</button>
          <button type="button" class="ts-quiz-button" data-answer="fails">Type error</button>
        </div>
        <div class="ts-quiz-reveal" hidden></div>
      </div>`;
    wireCommon(slot, codes);
    const reveal = slot.querySelector(".ts-quiz-reveal");
    for (const button of slot.querySelectorAll("[data-answer]")) {
      button.addEventListener("click", () => {
        const right = button.dataset.answer === (fails ? "fails" : "passes");
        const note = fails
          ? "The checker rejects it."
          : example.improves
            ? "The checker accepts it, and this version has earned it."
            : "The checker accepts it. Accepted is not the same as correct: " +
              "the weakness is real, the type just cannot see it.";
        reveal.innerHTML = `
          <p><strong>${right ? "Right." : "Not quite."}</strong>
            ${escape(example.title)}. ${note}</p>
          ${fails ? checkerWords(id) : ""}
          ${at + 1 < ids.length ? '<button type="button" class="ts-quiz-button ts-quiz-next">Next one</button>' : ""}`;
        reveal.hidden = false;
        for (const b of slot.querySelectorAll("[data-answer]")) {
          b.disabled = true;
        }
        slot.querySelector(".ts-quiz-next")?.addEventListener("click", () => {
          at += 1;
          render();
        });
      });
    }
  };
  rerenders.push(render);
  render();
}

// Selecting text highlights every identical occurrence on the page, so a
// selected identifier shows all its uses across the examples. Uses the CSS
// Custom Highlight API; a browser without it keeps plain selection.
if (typeof Highlight !== "undefined" && CSS.highlights) {
  const NAME = "ts-same-text";
  const CAP = 2000;
  let pending = 0;
  const main = document.querySelector("main");

  const repaint = () => {
    CSS.highlights.delete(NAME);
    const selection = document.getSelection();
    if (!selection || selection.isCollapsed) {
      return;
    }
    const needle = selection.toString().trim();
    if (needle.length < 2 || needle.length > 200 || needle.includes("\n")) {
      return;
    }
    const ranges = [];
    const walker = document.createTreeWalker(main, NodeFilter.SHOW_TEXT);
    outer: while (walker.nextNode()) {
      const node = walker.currentNode;
      let from = 0;
      for (;;) {
        const at = node.data.indexOf(needle, from);
        if (at < 0) {
          break;
        }
        const range = new Range();
        range.setStart(node, at);
        range.setEnd(node, at + needle.length);
        ranges.push(range);
        from = at + needle.length;
        if (ranges.length >= CAP) {
          break outer;
        }
      }
    }
    // One match is just the selection itself; echoes start at two.
    if (ranges.length > 1) {
      CSS.highlights.set(NAME, new Highlight(...ranges));
    }
  };

  document.addEventListener("selectionchange", () => {
    cancelAnimationFrame(pending);
    pending = requestAnimationFrame(repaint);
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
