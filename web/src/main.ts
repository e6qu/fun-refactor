/**
 * The playground.
 *
 * A workspace — the bundled sample, or a public repository fetched from GitHub — is
 * held in memory, handed to the analysis compiled as WebAssembly, and edited in
 * Monaco. There is no server: the tab holds the whole thing, and a refactoring here is
 * a real edit against real bytes that happens to be thrown away when you close it.
 *
 * The actions are built from `actions.ts` and the answers rendered by `render.ts`, so
 * this file is the wiring: what is open, where the cursor is, what an action needs
 * before it can run, and what to do with the workspace once one has changed it.
 */

// Monaco ships a syntax mode for every language it knows, but Vite code-splits each
// into its own chunk that the editor fetches only when a file needs it. What loads up
// front is the editor core.
import * as monaco from "monaco-editor";
import editorWorker from "monaco-editor/editor/editor.worker.start.js?worker";
import init, { Workspace } from "./wasm/fun_refactor.js";
import wasmUrl from "./wasm/fun_refactor_bg.wasm?url";
import { loadRepository, parseTarget } from "./github";
import { ACTIONS, GROUPS, type Action, type Context } from "./actions";
import { escapeHtml, render } from "./render";
import { installShell, setResizeHandler } from "./shell";
import { patchOf } from "./patch";
import "./style.css";

// Monaco wants a worker per language service. Only the core editor is loaded here —
// no TypeScript or JSON service, because the analysis *is* the language service — so
// one worker answers everything. Monaco 0.56 renamed the ESM entry to
// `editor.worker.start.js`; the old path resolves to nothing and fails at build.
self.MonacoEnvironment = { getWorker: () => new editorWorker() };

// The bundled sample: a small service in all fifteen languages, so every capability
// can be tried before deciding whether to wait for a repository to download — and so
// the page still works when GitHub rate-limits an anonymous browser.
const SAMPLE = import.meta.glob("../sample/**/*", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** A repository worth trying, chosen so each language has one that is small enough. */
const PRESETS: { label: string; target: string }[] = [
  { label: "Rust — ripgrep (crates/cli)", target: "BurntSushi/ripgrep/tree/master/crates/cli" },
  { label: "Go — helm (pkg/action)", target: "helm/helm/tree/main/pkg/action" },
  { label: "TypeScript — zod (src)", target: "colinhacks/zod/tree/main/packages/zod/src" },
  { label: "Python — requests (src)", target: "psf/requests/tree/main/src/requests" },
  { label: "Zig — zls (src)", target: "zigtools/zls/tree/master/src" },
  { label: "Bash — bats-core (lib)", target: "bats-core/bats-core/tree/master/lib" },
  { label: "HCL — terraform-aws-vpc", target: "terraform-aws-modules/terraform-aws-vpc" },
  { label: "Helm — ingress-nginx chart", target: "kubernetes/ingress-nginx/tree/main/charts/ingress-nginx" },
  { label: "CSS and HTML — normalize.css", target: "necolas/normalize.css" },
  { label: "Markdown — the Rust book (src)", target: "rust-lang/book/tree/main/src" },
];

const el = <T extends HTMLElement>(id: string): T => document.getElementById(id) as T;

const status = el<HTMLSpanElement>("status");
const result = el<HTMLDivElement>("result");
const fileList = el<HTMLUListElement>("file-list");
const fileCount = el<HTMLSpanElement>("file-count");
const openPath = el<HTMLSpanElement>("open-path");
const cursorLabel = el<HTMLSpanElement>("cursor");
const languageChip = el<HTMLSpanElement>("language");
const subjectLabel = el<HTMLSpanElement>("subject");
const outline = el<HTMLUListElement>("outline");
const outlineCount = el<HTMLSpanElement>("outline-count");
const actionList = el<HTMLDivElement>("action-list");
const undoButton = el<HTMLButtonElement>("undo");
const downloadButton = el<HTMLButtonElement>("download");
const astPane = el<HTMLDivElement>("ast");
const astCount = el<HTMLSpanElement>("ast-count");
const coordinate = el<HTMLButtonElement>("coordinate");
const nodeKind = el<HTMLSpanElement>("node-kind");

let workspace: Workspace | null = null;
let files: Record<string, string> = {};
/** What was loaded, before anything here changed it. */
let original: Record<string, string> = {};
let current = "";
let workspaceName = "sample";
const models = new Map<string, monaco.editor.ITextModel>();

/** Monaco's id for a path, so a loaded repository is highlighted correctly. */
function monacoLanguage(path: string): string {
  const name = path.split("/").pop() ?? "";
  const ext = name.includes(".") ? name.split(".").pop()!.toLowerCase() : "";
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

function say(text: string, kind: "" | "error" | "busy" = "") {
  status.textContent = text;
  status.className = `status ${kind}`;
}

function show(html: string, of = "Result") {
  el<HTMLSpanElement>("result-head").textContent = of;
  result.innerHTML = html;
  result.scrollTop = 0;
}

// ------------------------------------------------------------------ the cursor

/** The identifier under the cursor, so an action can say what it is about. */
function subjectAt(): string {
  const model = editor.getModel();
  const position = editor.getPosition();
  if (!model || !position) return "";
  return model.getWordAtPosition(position)?.word ?? "";
}

/** `line:col-line:col`, or null when nothing is selected. */
function selectionRange(): string | null {
  const s = editor.getSelection();
  if (!s || s.isEmpty()) return null;
  return `${s.startLineNumber}:${s.startColumn}-${s.endLineNumber}:${s.endColumn}`;
}

/** The language the analysis recognised the open file as. */
function languageOfOpenFile(): string {
  if (!workspace || !current) return "";
  const listed: any[] = JSON.parse(workspace.files());
  return listed.find((f) => f.path === current)?.language ?? "";
}

function context(answer: string): Context {
  const position = editor.getPosition();
  return {
    path: current,
    line: position?.lineNumber ?? 1,
    col: position?.column ?? 1,
    range: selectionRange(),
    subject: subjectAt(),
    answer,
    language: languageOfOpenFile(),
  };
}

/**
 * The status bar.
 *
 * The coordinate is the point of it: `path:line:col` is exactly what `fr` takes as a
 * target on the command line, so the same thing you are pointing at with a cursor can
 * be named in a terminal. Clicking it copies it.
 */
function refreshCursor() {
  const position = editor.getPosition();
  cursorLabel.textContent = position ? `Ln ${position.lineNumber}, Col ${position.column}` : "";
  const range = selectionRange();

  if (!workspace || !current || !position) {
    coordinate.textContent = "—";
    subjectLabel.textContent = "";
    nodeKind.textContent = "";
    updateAvailability();
    return;
  }

  let here: any = {};
  try {
    here = JSON.parse(workspace.at(current, position.lineNumber, position.column));
  } catch {
    here = {};
  }
  coordinate.textContent = here.coordinate ?? `${current}:${position.lineNumber}:${position.column}`;
  coordinate.title = `Copy — this is what \`fr\` accepts as a target`;
  nodeKind.textContent = here.node ?? "";

  subjectLabel.textContent = range
    ? `${range.replace("-", " → ")} selected`
    : here.name
      ? `${here.kind} ${here.qualifier ? here.qualifier + "::" : ""}${here.name}` +
        (here.exported ? " · exported" : "") +
        (here.definition && !here.definition.startsWith(`${current}:${position.lineNumber}`)
          ? ` · defined at ${here.definition}`
          : "")
      : "nothing the index knows at this position";

  highlightAstAt(position.lineNumber, position.column);
  updateAvailability();
}

coordinate.addEventListener("click", async () => {
  const text = coordinate.textContent ?? "";
  if (!text || text === "—") return;
  // The clipboard is unavailable over plain http on any origin but localhost, and
  // `writeText` rejects when the page is not focused. Saying "copied" either way
  // would be a lie about the one thing this button exists to do.
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    show(
      `<p class="err">This browser would not give the page the clipboard.</p>` +
        `<p class="hint">The coordinate is <code>${escapeHtml(text)}</code> — ` +
        `select it from here instead.</p>`,
      "Coordinate",
    );
    return;
  }
  coordinate.textContent = "copied";
  setTimeout(() => (coordinate.textContent = text), 900);
});

editor.onDidChangeCursorPosition(refreshCursor);
editor.onDidChangeCursorSelection(refreshCursor);

// ------------------------------------------------------------------- the files

function openFile(path: string) {
  if (!workspace || files[path] === undefined) return;
  current = path;
  let model = models.get(path);
  if (!model) {
    model = monaco.editor.createModel(files[path], monacoLanguage(path));
    models.set(path, model);
  }
  editor.setModel(model);
  openPath.textContent = path;
  languageChip.textContent = languageOfOpenFile() || "not indexed";
  for (const li of fileList.querySelectorAll("li")) {
    li.classList.toggle("on", li.dataset.path === path);
  }
  renderOutline();
  renderAst();
  refreshCursor();
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

/** The open file's definitions, as a jump list beside it. */
function renderOutline() {
  outline.innerHTML = "";
  if (!workspace || !current) {
    outlineCount.textContent = "";
    return;
  }
  let symbols: any[] = [];
  try {
    const parsed = JSON.parse(workspace.symbols(current));
    symbols = Array.isArray(parsed) ? parsed : [];
  } catch {
    symbols = [];
  }
  outlineCount.textContent = symbols.length ? String(symbols.length) : "none";
  for (const symbol of symbols) {
    const li = document.createElement("li");
    li.innerHTML =
      `<span class="chip">${escapeHtml(symbol.kind)}</span> ${escapeHtml(symbol.name)}`;
    li.title = `${symbol.kind} ${symbol.name} — line ${symbol.line}`;
    li.addEventListener("click", () => {
      const at = { lineNumber: symbol.line, column: symbol.col };
      editor.setPosition(at);
      editor.revealPositionInCenter(at);
      editor.focus();
    });
    outline.appendChild(li);
  }
}

// ------------------------------------------------------------------ structure

/** Every node currently rendered, so the cursor can be followed into the tree. */
let astRows: { node: any; row: HTMLElement }[] = [];

/**
 * The parse tree of the open file.
 *
 * Rendered lazily below a depth: a file of any size has tens of thousands of nodes,
 * and building them all costs more than anyone wants to pay for a pane they may not
 * look at. Deeper levels appear when their parent is opened.
 */
function renderAst() {
  astPane.innerHTML = "";
  astRows = [];
  if (!workspace || !current) {
    astCount.textContent = "";
    return;
  }
  let tree: any;
  try {
    tree = JSON.parse(workspace.ast(current));
  } catch (e) {
    astPane.innerHTML = `<p class="pane-note err">${escapeHtml(String(e))}</p>`;
    astCount.textContent = "";
    return;
  }
  if (tree.error) {
    astPane.innerHTML = `<p class="pane-note err">${escapeHtml(tree.error)}</p>`;
    astCount.textContent = "";
    return;
  }
  astCount.textContent = `${countNodes(tree)} nodes`;
  astPane.appendChild(astRow(tree, 0, true));
}

function countNodes(node: any): number {
  return 1 + node.children.reduce((n: number, c: any) => n + countNodes(c), 0);
}

function astRow(node: any, depth: number, open: boolean): HTMLElement {
  const wrap = document.createElement("div");
  wrap.className = "ast-node";

  const row = document.createElement("div");
  row.className = "ast-row";
  row.style.paddingLeft = `${0.4 + depth * 0.7}rem`;
  const hasChildren = node.children.length > 0;
  row.innerHTML =
    `<span class="ast-twisty">${hasChildren ? (open ? "▾" : "▸") : "·"}</span>` +
    (node.field ? `<span class="ast-field">${escapeHtml(node.field)}:</span> ` : "") +
    `<span class="ast-kind">${escapeHtml(node.kind)}</span>` +
    (node.text !== null && node.text !== undefined
      ? ` <span class="ast-text">${escapeHtml(node.text)}</span>`
      : "");
  row.title = `${node.kind} — ${node.line}:${node.col} to ${node.end_line}:${node.end_col}`;

  const kids = document.createElement("div");
  kids.className = "ast-kids";
  kids.hidden = !open;
  let built = false;
  const build = () => {
    if (built) return;
    built = true;
    for (const child of node.children) kids.appendChild(astRow(child, depth + 1, false));
  };
  if (open) build();

  row.addEventListener("click", (e) => {
    // Selecting what the node covers is the useful half: it is exactly the range
    // `extract` would take, so the tree doubles as a way to pick one.
    editor.setSelection({
      startLineNumber: node.line,
      startColumn: node.col,
      endLineNumber: node.end_line,
      endColumn: node.end_col,
    });
    editor.revealLineInCenterIfOutsideViewport(node.line);
    if (hasChildren && (e.target as HTMLElement).classList.contains("ast-twisty")) {
      build();
      kids.hidden = !kids.hidden;
      row.querySelector(".ast-twisty")!.textContent = kids.hidden ? "▸" : "▾";
    }
  });

  wrap.appendChild(row);
  wrap.appendChild(kids);
  astRows.push({ node, row });
  return wrap;
}

/** Mark the deepest rendered node covering the cursor. */
function highlightAstAt(line: number, col: number) {
  let best: { node: any; row: HTMLElement } | null = null;
  for (const entry of astRows) {
    const n = entry.node;
    const after = line > n.line || (line === n.line && col >= n.col);
    const before = line < n.end_line || (line === n.end_line && col <= n.end_col);
    if (after && before) {
      if (!best || n.line > best.node.line || countNodes(n) < countNodes(best.node)) best = entry;
    }
    entry.row.classList.remove("on");
  }
  best?.row.classList.add("on");
}

/** Re-read every model from the workspace after an edit changed the bytes. */
function syncFromWorkspace(changed: { path: string }[]) {
  if (!workspace) return;
  let listChanged = false;
  for (const { path } of changed) {
    const text = workspace.read(path);
    if (files[path] === undefined) listChanged = true;
    files[path] = text;
    const model = models.get(path);
    if (model && model.getValue() !== text) {
      // `pushEditOperations` rather than `setValue` so undo still works and the
      // viewport does not jump.
      model.pushEditOperations([], [{ range: model.getFullModelRange(), text }], () => null);
    }
  }
  if (listChanged) renderFileList(el<HTMLInputElement>("filter").value);
  renderOutline();
  renderAst();
  // The bytes under the cursor have changed, so what the status bar says about them
  // is now about the old text.
  refreshCursor();
  refreshEditedState();
}

function editedPaths(): string[] {
  return Object.keys(files).filter((p) => files[p] !== original[p]);
}

function refreshEditedState() {
  const edited = editedPaths();
  undoButton.disabled = edited.length === 0;
  downloadButton.disabled = edited.length === 0;
  undoButton.textContent = edited.length
    ? `Undo ${edited.length} edited file${edited.length === 1 ? "" : "s"}`
    : "Undo all edits";
}

// ----------------------------------------------------------------- the actions

/**
 * Ask for one value, in a dialog rather than a `prompt` the browser may block.
 *
 * Settled from the form's own `submit` rather than from the dialog's `close`. A
 * `<form method="dialog">` closes its dialog and sets `returnValue` without always
 * firing `close` — which left the action waiting forever on a promise that had no
 * remaining way to settle, with the dialog gone and nothing said. `submit` fires
 * whichever way the form is completed, and Escape is covered by `cancel`.
 */
function ask(action: Action, prefill: string): Promise<string | null> {
  const dialog = el<HTMLDialogElement>("ask");
  const form = el<HTMLFormElement>("ask-form");
  el<HTMLHeadingElement>("ask-title").textContent = action.label;
  el<HTMLParagraphElement>("ask-why").textContent = action.ask?.why ?? action.describes;
  el<HTMLLabelElement>("ask-label").textContent = action.ask!.label;
  el<HTMLParagraphElement>("ask-example").textContent = `for example: ${action.ask!.example}`;
  const input = el<HTMLInputElement>("ask-value");
  input.value = prefill;
  input.placeholder = action.ask!.example;

  return new Promise((resolve) => {
    let settled = false;
    const finish = (value: string | null) => {
      if (settled) return;
      settled = true;
      form.removeEventListener("submit", onSubmit);
      dialog.removeEventListener("cancel", onCancel);
      dialog.removeEventListener("close", onClose);
      if (dialog.open) dialog.close();
      resolve(value);
    };
    const onSubmit = (e: SubmitEvent) => {
      const wanted = (e.submitter as HTMLButtonElement | null)?.value === "ok";
      const typed = input.value.trim();
      finish(wanted && typed ? typed : null);
    };
    const onCancel = () => finish(null);
    const onClose = () =>
      finish(dialog.returnValue === "ok" && input.value.trim() ? input.value.trim() : null);

    form.addEventListener("submit", onSubmit);
    dialog.addEventListener("cancel", onCancel);
    dialog.addEventListener("close", onClose);
    dialog.showModal();
    input.select();
  });
}

function unavailable(action: Action): string | null {
  if (!workspace) return "Load a workspace first.";
  if (action.needs === "workspace") return null;
  if (!current) return "Open a file first.";
  if (action.needs === "file") return null;
  if (action.needs === "selection") {
    return selectionRange() ? null : "Select the code to lift out first.";
  }
  return editor.getPosition() ? null : "Put the cursor on a name first.";
}

async function run(action: Action) {
  const why = unavailable(action);
  if (why) {
    show(`<p class="hint">${escapeHtml(why)}</p>`, action.label);
    return;
  }

  let answer = "";
  if (action.ask) {
    const prefill = action.ask.fromSubject ? subjectAt() : "";
    const given = await ask(action, prefill);
    if (given === null) return;
    answer = given;
  }

  show(`<p class="hint">Working…</p>`, action.label);
  let value: any;
  try {
    value = JSON.parse(action.run(workspace!, context(answer)));
  } catch (e) {
    show(`<p class="err">${escapeHtml(String(e))}</p>`, action.label);
    return;
  }

  if (value && typeof value === "object" && "error" in value) {
    // A refusal is an answer: it says which rule stopped the change, and that is
    // the thing worth reading.
    show(
      `<p class="err">${escapeHtml(String(value.error))}</p>` +
        `<p class="hint">The tool refuses rather than guessing. Nothing was changed.</p>`,
      action.label,
    );
    return;
  }

  if (action.mutates && value?.files) syncFromWorkspace(value.files);
  show(render(value, current, action.empty), action.label);
}

function updateAvailability() {
  for (const button of actionList.querySelectorAll<HTMLButtonElement>("button[data-act]")) {
    const action = ACTIONS.find((a) => a.id === button.dataset.act)!;
    const why = unavailable(action);
    button.disabled = why !== null;
    button.title = why ?? action.describes;
  }
}

function buildActions(filter = "") {
  const needle = filter.trim().toLowerCase();
  actionList.innerHTML = "";
  for (const group of GROUPS) {
    const inGroup = ACTIONS.filter(
      (a) =>
        a.group === group &&
        (!needle ||
          a.label.toLowerCase().includes(needle) ||
          a.describes.toLowerCase().includes(needle)),
    );
    if (!inGroup.length) continue;

    const section = document.createElement("section");
    section.className = "group";
    section.innerHTML = `<h3>${escapeHtml(group)}</h3>`;
    for (const action of inGroup) {
      const button = document.createElement("button");
      button.dataset.act = action.id;
      button.innerHTML =
        `<span class="act-label">${escapeHtml(action.label)}` +
        (action.mutates ? ` <span class="tier edits">edits</span>` : "") +
        `</span><span class="act-note">${escapeHtml(action.describes)}</span>`;
      button.addEventListener("click", () => void run(action));
      section.appendChild(button);
    }
    actionList.appendChild(section);
  }
  updateAvailability();
}

// ------------------------------------------------------------------ navigation

result.addEventListener("click", (e) => {
  const target = (e.target as HTMLElement).closest("[data-goto]");
  if (!target) return;
  const raw = (target as HTMLElement).dataset.goto!;
  // A path can contain a colon on no platform we load from, but the position is
  // always the last two fields, so split from the right.
  const parts = raw.split(":");
  const col = Number(parts.pop());
  const line = Number(parts.pop());
  const path = parts.join(":");
  if (path && path !== current) openFile(path);
  const at = { lineNumber: line, column: col };
  editor.setPosition(at);
  editor.revealPositionInCenter(at);
  editor.focus();
});

el<HTMLInputElement>("filter").addEventListener("input", (e) => {
  renderFileList((e.target as HTMLInputElement).value);
});

el<HTMLInputElement>("action-filter").addEventListener("input", (e) => {
  buildActions((e.target as HTMLInputElement).value);
});

// --------------------------------------------------------------------- loading

function adopt(loaded: Record<string, string>, name: string) {
  // Undoing re-indexes the original bytes, which is the same code path as loading a
  // repository. What it must not do is move you: you were reading a file when you
  // pressed it, and coming back somewhere else is disorienting.
  const wasOpen = current;
  files = { ...loaded };
  original = { ...loaded };
  workspaceName = name;
  models.forEach((m) => m.dispose());
  models.clear();
  current = "";
  workspace = new Workspace(files);

  renderFileList(el<HTMLInputElement>("filter").value);
  // Otherwise open something worth looking at: the largest indexed file beats
  // whichever path happens to sort first, which is usually a config file.
  const listed: any[] = JSON.parse(workspace.files());
  const best = listed
    .filter((f) => f.indexed)
    .sort((a, b) => (files[b.path]?.length ?? 0) - (files[a.path]?.length ?? 0))[0];
  const open = files[wasOpen] !== undefined ? wasOpen : best?.path;
  if (open) openFile(open);
  refreshEditedState();
}

function loadSample() {
  const loaded: Record<string, string> = {};
  for (const [key, text] of Object.entries(SAMPLE)) {
    loaded[key.replace("../sample/", "")] = text;
  }
  say(`Indexing the bundled sample (${Object.keys(loaded).length} files)…`, "busy");
  adopt(loaded, "sample");
  say("Bundled sample — fifteen languages, no network needed.");
  show(render(JSON.parse(workspace!.stats()), current), "Index stats");
}

el<HTMLButtonElement>("load-sample").addEventListener("click", loadSample);

const presetSelect = el<HTMLSelectElement>("preset");
for (const preset of PRESETS) {
  const option = document.createElement("option");
  option.value = preset.target;
  option.textContent = preset.label;
  presetSelect.appendChild(option);
}
presetSelect.addEventListener("change", () => {
  if (!presetSelect.value) return;
  el<HTMLInputElement>("target").value = presetSelect.value;
  presetSelect.selectedIndex = 0;
  el<HTMLFormElement>("load-form").requestSubmit();
});

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
      onProgress: (done, total, note) => say(`${done}/${total} ${note}`, "busy"),
    });

    say(`Indexing ${Object.keys(loaded.files).length} files…`, "busy");
    adopt(loaded.files, `${target.owner}/${target.repo}`);
    show(render(JSON.parse(workspace!.stats()), current), "Index stats");

    const notes: string[] = [];
    if (loaded.skipped.length) {
      notes.push(`${loaded.skipped.length} file(s) left out — see the console`);
      console.info("[fun-refactor] left out:", loaded.skipped);
    }
    if (loaded.truncatedTree) notes.push("GitHub truncated the file listing");
    say(
      `${target.owner}/${target.repo}@${loaded.ref}` +
        (notes.length ? ` · ${notes.join(" · ")}` : ""),
    );
  } catch (error) {
    say(String(error instanceof Error ? error.message : error), "error");
  } finally {
    button.disabled = false;
  }
});

// ----------------------------------------------------------------- the results

undoButton.addEventListener("click", () => {
  if (!editedPaths().length) return;
  say("Reindexing the original files…", "busy");
  adopt(original, workspaceName);
  say(`${workspaceName} — back to what was loaded.`);
  show(`<p class="hint">Every edit undone. The workspace is what was loaded.</p>`, "Undo");
});

downloadButton.addEventListener("click", () => {
  const edited = editedPaths();
  if (!edited.length) return;
  const patch = patchOf(edited, original, files);
  const blob = new Blob([patch], { type: "text/x-patch" });
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = `${workspaceName.replace(/[^\w.-]+/g, "-")}.patch`;
  link.click();
  URL.revokeObjectURL(link.href);
});

// ----------------------------------------------------------------------- start

installShell();
setResizeHandler(() => editor.layout());
buildActions();
say("Loading the analysis…", "busy");
init({ module_or_path: wasmUrl })
  .then(() => {
    el<HTMLButtonElement>("load").disabled = false;
    // Start with something loaded. A page whose every button is greyed out until you
    // have thought of a repository is a page nobody tries.
    loadSample();
  })
  .catch((e) => say(`The analysis failed to load: ${e}`, "error"));
