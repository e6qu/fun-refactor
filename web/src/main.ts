/**
 * The playground.
 *
 * A public repository is fetched from GitHub into memory, handed to the analysis
 * compiled as WebAssembly, and edited in Monaco. There is no server: the tab holds
 * the whole workspace, and a refactoring here is a real edit against real bytes that
 * happens to be thrown away when you close it.
 *
 * What the wasm returns is JSON — the same shapes `fr --json` prints — so this file
 * is a view over answers rather than a second implementation of them.
 */

// Monaco ships a syntax mode for every language it knows, but Vite code-splits each
// into its own chunk that the editor fetches only when a file needs it. What loads up
// front is the editor core.
import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/editor/editor.worker.start.js?worker";
import init, { Workspace } from "./wasm/fun_refactor.js";
import wasmUrl from "./wasm/fun_refactor_bg.wasm?url";
import { loadRepository, parseTarget } from "./github";
import "./style.css";

// Monaco wants a worker per language service. Only the core editor is loaded here —
// no TypeScript or JSON service, because the analysis *is* the language service — so
// one worker answers everything. Monaco 0.56 renamed the ESM entry to
// `editor.worker.start.js`; the old path resolves to nothing and fails at build.
self.MonacoEnvironment = { getWorker: () => new editorWorker() };

const el = <T extends HTMLElement>(id: string): T =>
  document.getElementById(id) as T;

const status = el<HTMLSpanElement>("status");
const result = el<HTMLDivElement>("result");
const fileList = el<HTMLUListElement>("file-list");
const fileCount = el<HTMLSpanElement>("file-count");
const openPath = el<HTMLSpanElement>("open-path");
const cursorLabel = el<HTMLSpanElement>("cursor");

let workspace: Workspace | null = null;
let files: Record<string, string> = {};
let current = "";
const models = new Map<string, monaco.editor.ITextModel>();

/** Monaco's id for a path, so a loaded repository is highlighted correctly. */
function monacoLanguage(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    rs: "rust", go: "go", ts: "typescript", tsx: "typescript",
    js: "javascript", jsx: "javascript", py: "python", zig: "cpp",
    sh: "shell", bash: "shell", html: "html", htm: "html",
    css: "css", scss: "scss", tf: "hcl", tfvars: "hcl",
    yaml: "yaml", yml: "yaml", xml: "xml", md: "markdown",
  };
  return map[ext] ?? "plaintext";
}

const editor = monaco.editor.create(el("editor"), {
  value: "",
  language: "plaintext",
  automaticLayout: true,
  minimap: { enabled: false },
  fontSize: 13,
  scrollBeyondLastLine: false,
  renderWhitespace: "selection",
  theme: matchMedia("(prefers-color-scheme: dark)").matches ? "vs-dark" : "vs",
});

editor.onDidChangeCursorPosition((e) => {
  cursorLabel.textContent = `${e.position.lineNumber}:${e.position.column}`;
});

function say(text: string, kind: "" | "error" | "busy" = "") {
  status.textContent = text;
  status.className = `status ${kind}`;
}

function show(html: string) {
  result.innerHTML = html;
}

function escapeHtml(text: string): string {
  return text.replace(/[&<>"]/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] as string);
}

/** JSON from the wasm, or an object with `error` when it refused. */
function call<T>(json: string): T | null {
  const value = JSON.parse(json);
  if (value && typeof value === "object" && "error" in value) {
    show(`<p class="err">${escapeHtml(String(value.error))}</p>`);
    return null;
  }
  return value as T;
}

function openFile(path: string) {
  if (!workspace) return;
  current = path;
  let model = models.get(path);
  if (!model) {
    model = monaco.editor.createModel(files[path] ?? "", monacoLanguage(path));
    models.set(path, model);
  }
  editor.setModel(model);
  openPath.textContent = path;
  for (const li of fileList.querySelectorAll("li")) {
    li.classList.toggle("on", li.dataset.path === path);
  }
}

function renderFileList(filter = "") {
  const paths = Object.keys(files)
    .filter((p) => p.toLowerCase().includes(filter.toLowerCase()))
    .sort();
  fileCount.textContent = `${paths.length} file${paths.length === 1 ? "" : "s"}`;
  fileList.innerHTML = "";
  for (const path of paths) {
    const li = document.createElement("li");
    li.dataset.path = path;
    li.textContent = path;
    li.title = path;
    li.classList.toggle("on", path === current);
    li.addEventListener("click", () => openFile(path));
    fileList.appendChild(li);
  }
}

/** Re-read every model from the workspace after an edit changed the bytes. */
function syncFromWorkspace(changed: { path: string }[]) {
  if (!workspace) return;
  for (const { path } of changed) {
    const text = workspace.read(path);
    files[path] = text;
    const model = models.get(path);
    if (model && model.getValue() !== text) {
      // `pushEditOperations` rather than `setValue` so undo still works and the
      // viewport does not jump.
      model.pushEditOperations(
        [],
        [{ range: model.getFullModelRange(), text }],
        () => null,
      );
    }
  }
}

function position(): { path: string; line: number; col: number } | null {
  const pos = editor.getPosition();
  if (!current || !pos) {
    show(`<p class="hint">Open a file and put the cursor on a name first.</p>`);
    return null;
  }
  return { path: current, line: pos.lineNumber, col: pos.column };
}

// ------------------------------------------------------------------- actions

const actions: Record<string, () => void> = {
  stats() {
    if (!workspace) return;
    const s = call<any>(workspace.stats());
    if (!s) return;
    show(`
      <dl class="stats">
        <dt>files indexed</dt><dd>${s.files}</dd>
        <dt>symbols</dt><dd>${s.symbols}</dd>
        <dt>references</dt><dd>${s.references}</dd>
      </dl>
      <ul class="plain">${s.languages
        .map(([name, n]: [string, number]) => `<li>${escapeHtml(name)} <span class="dim">${n}</span></li>`)
        .join("")}</ul>
      ${s.unparsed.length ? `<p class="warn">${s.unparsed.length} file(s) did not parse, so references in them are not counted.</p>` : ""}
    `);
  },

  definition() {
    const at = position();
    if (!at || !workspace) return;
    const found = call<any>(workspace.definition(at.path, at.line, at.col));
    if (!found) return;
    const items = (found.definitions ?? []).map(
      (d: any) =>
        `<li><a data-goto="${escapeHtml(d.location.file)}:${d.location.line}:${d.location.col}">${escapeHtml(d.location.file)}:${d.location.line}</a> <span class="dim">${escapeHtml(d.role ?? "")}</span></li>`,
    );
    show(items.length
      ? `<ul class="plain">${items.join("")}</ul>`
      : `<p class="hint">Nothing is defined at that position.</p>`);
  },

  references() {
    const at = position();
    if (!at || !workspace) return;
    const refs = call<any[]>(workspace.references(at.path, at.line, at.col));
    if (!refs) return;
    if (!refs.length) {
      show(`<p class="hint">No references resolve to that symbol.</p>`);
      return;
    }
    show(`<p class="count">${refs.length} reference(s)</p><ul class="plain">${refs
      .map(
        (r) =>
          `<li><a data-goto="${escapeHtml(r.path)}:${r.line}:${r.col}">${escapeHtml(r.path)}:${r.line}:${r.col}</a> <span class="tier ${r.confidence}">${escapeHtml(r.confidence)}</span></li>`,
      )
      .join("")}</ul>`);
  },

  rename() {
    const at = position();
    if (!at || !workspace) return;
    const name = el<HTMLInputElement>("new-name").value.trim();
    if (!name) {
      show(`<p class="hint">Type the new name first.</p>`);
      return;
    }
    const applied = call<any>(workspace.rename(at.path, at.line, at.col, name));
    if (!applied) return;
    syncFromWorkspace(applied.files);
    const diffs = applied.files
      .map((f: any) => `<pre class="diff">${colourDiff(f.diff)}</pre>`)
      .join("");
    const warnings = (applied.warnings ?? []).length
      ? `<p class="warn">Not changed — review these yourself:</p><ul class="plain">${applied.warnings
          .map((w: any) => `<li>${escapeHtml(w.file ?? "")} <span class="dim">${escapeHtml(w.detail ?? w.kind ?? "")}</span></li>`)
          .join("")}</ul>`
      : "";
    show(`<p class="count">${applied.files.length} file(s) changed</p>${diffs}${warnings}`);
  },

  removeFlag() {
    if (!workspace) return;
    const flag = el<HTMLInputElement>("flag-name").value.trim();
    if (!flag) {
      show(`<p class="hint">Name the flag to retire.</p>`);
      return;
    }
    // Retiring a flag means deciding what it always was. `true` is the common case —
    // the feature shipped and the switch is what is left over.
    const applied = call<any>(workspace.remove_flag(flag, true));
    if (!applied) return;
    syncFromWorkspace(applied.files);
    if (!applied.files.length) {
      show(`<p class="hint">Nothing references <code>${escapeHtml(flag)}</code>.</p>`);
      return;
    }
    renderFileList(el<HTMLInputElement>("filter").value);
    show(`<p class="count">${applied.files.length} file(s) changed</p>` +
      applied.files.map((f: any) => `<pre class="diff">${colourDiff(f.diff)}</pre>`).join(""));
  },

  duplicates() {
    if (!workspace) return;
    const classes = call<any[]>(workspace.duplicates(60));
    if (!classes) return;
    if (!classes.length) {
      show(`<p class="hint">No duplication of 60 tokens or more.</p>`);
      return;
    }
    show(classes
      .slice(0, 20)
      .map(
        (c) =>
          `<p class="count">${c.instances.length} copies, ${c.tokens} tokens each</p><ul class="plain">${c.instances
            .map((i: any) => `<li><a data-goto="${escapeHtml(i.file)}:${i.start_line}:1">${escapeHtml(i.file)}:${i.start_line}-${i.end_line}</a></li>`)
            .join("")}</ul>`,
      )
      .join(""));
  },

  unused() {
    if (!workspace) return;
    const dead = call<any[]>(workspace.unused());
    if (!dead) return;
    const internal = dead.filter((d) => !d.exported);
    show(`<p class="count">${dead.length} with no detected use, ${internal.length} of them unexported</p>
      <ul class="plain">${dead
        .slice(0, 60)
        .map(
          (d) =>
            `<li>${escapeHtml(d.kind)} <strong>${escapeHtml(d.name)}</strong> <span class="dim">${escapeHtml(d.path)}</span>${d.exported ? ' <span class="tier name-only">exported</span>' : ""}</li>`,
        )
        .join("")}</ul>
      <p class="hint">An exported symbol with no use here may be a public API rather than dead code.</p>`);
  },
};

function colourDiff(diff: string): string {
  return diff
    .split("\n")
    .map((line) => {
      const cls = line.startsWith("+++") || line.startsWith("---")
        ? "meta"
        : line.startsWith("+") ? "add"
        : line.startsWith("-") ? "del"
        : line.startsWith("@@") ? "meta" : "";
      return `<span class="${cls}">${escapeHtml(line)}</span>`;
    })
    .join("\n");
}

result.addEventListener("click", (e) => {
  const target = (e.target as HTMLElement).closest("[data-goto]");
  if (!target) return;
  const [path, line, col] = (target as HTMLElement).dataset.goto!.split(":");
  openFile(path);
  const at = { lineNumber: Number(line), column: Number(col) };
  editor.setPosition(at);
  editor.revealPositionInCenter(at);
  editor.focus();
});

for (const button of document.querySelectorAll<HTMLButtonElement>("[data-act]")) {
  button.addEventListener("click", () => {
    if (!workspace) {
      show(`<p class="hint">Load a repository first.</p>`);
      return;
    }
    try {
      actions[button.dataset.act!]();
    } catch (e) {
      show(`<p class="err">${escapeHtml(String(e))}</p>`);
    }
  });
}

el<HTMLInputElement>("filter").addEventListener("input", (e) => {
  renderFileList((e.target as HTMLInputElement).value);
});

// --------------------------------------------------------------------- load

el<HTMLFormElement>("load-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  const target = parseTarget(el<HTMLInputElement>("target").value);
  if (!target) {
    say("Give an owner/repo, or a github.com URL.", "error");
    return;
  }
  const button = el<HTMLButtonElement>("load");
  button.disabled = true;
  try {
    say(`Fetching ${target.owner}/${target.repo}…`, "busy");
    const loaded = await loadRepository({
      ...target,
      onProgress: (done, total, note) =>
        say(`${done}/${total} ${note}`, "busy"),
    });

    files = loaded.files;
    models.forEach((m) => m.dispose());
    models.clear();
    current = "";

    say(`Indexing ${Object.keys(files).length} files…`, "busy");
    workspace = new Workspace(files);

    renderFileList(el<HTMLInputElement>("filter").value);
    const first = Object.keys(files).sort()[0];
    if (first) openFile(first);

    const notes: string[] = [];
    if (loaded.skipped.length) {
      notes.push(`${loaded.skipped.length} file(s) left out — see the console`);
      console.info("[fun-refactor] left out:", loaded.skipped);
    }
    if (loaded.truncatedTree) {
      notes.push("GitHub truncated the file listing for this repository");
    }
    say(
      `${target.owner}/${target.repo}@${loaded.ref}` +
        (notes.length ? ` · ${notes.join(" · ")}` : ""),
    );
    actions.stats();
  } catch (error) {
    say(String(error instanceof Error ? error.message : error), "error");
  } finally {
    button.disabled = false;
  }
});

// ------------------------------------------------------------------- start

say("Loading the analysis…", "busy");
init({ module_or_path: wasmUrl })
  .then(() => {
    say("Ready. Load a repository to begin.");
    el<HTMLButtonElement>("load").disabled = false;
  })
  .catch((e) => say(`The analysis failed to load: ${e}`, "error"));
