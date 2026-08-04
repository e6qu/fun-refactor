/**
 * The playground, as a page you use.
 *
 * Loaded by `main.ts` when the page is being rendered for a person. The machine-
 * readable modes never import this file, so nothing here — Monaco above all — is
 * fetched or constructed for them.
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
import { decorate } from "./icons";
import { installTheme, onThemeChange, current as currentTheme, toggle as toggleTheme } from "./theme";
import { installHelp, openHelp, livePages, PAGES } from "./help";
import * as menu from "./menu";
import "./style.css";

// Monaco wants a worker per language service. Only the core editor is loaded here —
// no TypeScript or JSON service, because the analysis *is* the language service — so
// one worker answers everything. Monaco 0.56 renamed the ESM entry to
// `editor.worker.start.js`; the old path resolves to nothing and fails at build.
self.MonacoEnvironment = { getWorker: () => new editorWorker() };

// The bundled sample: a small service in all sixteen languages, so every capability
// can be tried before deciding whether to wait for a repository to download — and so
// the page still works when GitHub rate-limits an anonymous browser.
const SAMPLE = import.meta.glob("../sample/**/*", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/**
 * What loads if you do nothing.
 *
 * `psf/requests` is real, widely read, and the right size: nineteen files of Python
 * that fit in a tab and still have interfaces, inheritance, a `__init__.py` re-export
 * surface and genuine dead code. A synthetic sample can only demonstrate; this can
 * surprise you, which is the point of a playground.
 */
const DEFAULT_REPOSITORY = "psf/requests/tree/main/src/requests";

/** A repository worth trying, chosen so each language has one that is small enough. */
const PRESETS: { label: string; target: string }[] = [
  { label: "Python — requests (src)", target: DEFAULT_REPOSITORY },
  { label: "Python — httpx (src)", target: "encode/httpx/tree/master/httpx" },
  { label: "Rust — ripgrep (crates/cli)", target: "BurntSushi/ripgrep/tree/master/crates/cli" },
  { label: "Go — helm (pkg/action)", target: "helm/helm/tree/main/pkg/action" },
  { label: "TypeScript — zod (src)", target: "colinhacks/zod/tree/main/packages/zod/src" },
  { label: "Zig — zls (src)", target: "zigtools/zls/tree/master/src" },
  { label: "Bash — bats-core (lib)", target: "bats-core/bats-core/tree/master/lib" },
  { label: "HCL — terraform-aws-vpc", target: "terraform-aws-modules/terraform-aws-vpc" },
  { label: "Helm — ingress-nginx chart", target: "kubernetes/ingress-nginx/tree/main/charts/ingress-nginx" },
  { label: "CSS and HTML — normalize.css", target: "necolas/normalize.css" },
  { label: "Markdown — the Rust book (src)", target: "rust-lang/book/tree/main/src" },
  { label: "All sixteen languages — bundled sample", target: "sample" },
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
  menu.close();
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

/**
 * Grey out what cannot run, wherever it is offered.
 *
 * The toolbar icons and the two menus ask the same question of the same rule, so an
 * action can never be enabled in one place and refused in another.
 */
function updateAvailability() {
  for (const button of document.querySelectorAll<HTMLButtonElement>("#nav-icons button[data-act]")) {
    const action = ACTIONS.find((a) => a.id === button.dataset.act);
    if (!action) continue;
    const why = unavailable(action);
    button.disabled = why !== null;
    button.title = why ? `${button.dataset.label} — ${why}` : (button.dataset.label ?? "");
  }
}

// ------------------------------------------------------------------ navigation

/** Somewhere you have been, so you can get back to it. */
interface Place {
  path: string;
  line: number;
  col: number;
}

// A jump you cannot undo is a jump you hesitate to make. Every deliberate move goes
// on this stack; Alt+← and Alt+→ walk it, as they do in an editor.
const history: Place[] = [];
let historyAt = -1;

function where(): Place | null {
  const position = editor.getPosition();
  return current && position
    ? { path: current, line: position.lineNumber, col: position.column }
    : null;
}

/** Move the cursor there, without touching the history. */
function moveTo(place: Place) {
  if (place.path && place.path !== current) openFile(place.path);
  const at = { lineNumber: place.line, column: place.col };
  editor.setPosition(at);
  editor.revealPositionInCenter(at);
  editor.focus();
}

/** Move the cursor there, and remember where we were. */
function jumpTo(place: Place) {
  const from = where();
  if (from && (from.path !== place.path || from.line !== place.line)) {
    history.splice(historyAt + 1);
    history.push(from);
    historyAt = history.length - 1;
  }
  moveTo(place);
  refreshHistoryButtons();
}

/** `path:line:col`, as every result link spells it. */
function parsePlace(raw: string): Place {
  // A path can contain a colon on no platform we load from, but the position is
  // always the last two fields, so split from the right.
  const parts = raw.split(":");
  const col = Number(parts.pop());
  const line = Number(parts.pop());
  return { path: parts.join(":"), line, col };
}

function goBack() {
  if (historyAt < 0) return;
  const here = where();
  const place = history[historyAt];
  // Leave the place we are standing in where the forward step will find it.
  if (here) history[historyAt] = here;
  historyAt -= 1;
  moveTo(place);
  refreshHistoryButtons();
}

function goForward() {
  if (historyAt + 1 >= history.length) return;
  const here = where();
  historyAt += 1;
  const place = history[historyAt];
  if (here) history[historyAt] = here;
  moveTo(place);
  refreshHistoryButtons();
}

function refreshHistoryButtons() {
  const back = document.getElementById("nav-back") as HTMLButtonElement | null;
  const forward = document.getElementById("nav-forward") as HTMLButtonElement | null;
  if (back) back.disabled = historyAt < 0;
  if (forward) forward.disabled = historyAt + 1 >= history.length;
}

/** Go to the one place a symbol is defined, or report why that is not a question. */
function goToDefinition() {
  if (!workspace || !current) return;
  const position = editor.getPosition();
  if (!position) return;
  const found = JSON.parse(
    workspace.definition(current, position.lineNumber, position.column),
  );
  if (found.error || !found.definitions?.length) {
    show(
      `<p class="hint">${escapeHtml(
        found.error ?? "Nothing is defined at that position.",
      )}</p>`,
      "Go to definition",
    );
    return;
  }
  // Several definitions is a real answer — an abstraction with implementations — so
  // the list is shown and the first is jumped to.
  const primary = found.definitions.find((d: any) => d.role === "primary") ?? found.definitions[0];
  jumpTo({
    path: primary.location.file,
    line: primary.location.line,
    col: primary.location.col,
  });
  show(render(found, current), "Go to definition");
}

result.addEventListener("click", (e) => {
  const target = (e.target as HTMLElement).closest("[data-goto]");
  if (!target) return;
  jumpTo(parsePlace((target as HTMLElement).dataset.goto!));
});

el<HTMLInputElement>("filter").addEventListener("input", (e) => {
  renderFileList((e.target as HTMLInputElement).value);
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

  // A `Workspace` owns Rust memory that JavaScript's collector knows nothing about:
  // the whole index, every symbol and every reference. Dropping the last handle to it
  // frees nothing. Loading a second repository — or pressing Undo, which re-indexes —
  // therefore leaked an entire index each time, and on a repository of any size the
  // next allocation aborted the module with `unreachable`. In a tab that is not an
  // exception you can catch; it is the end of the workspace.
  workspace?.free();
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
  adopt(loaded, "bundled sample");
  say("Bundled sample — sixteen languages, no network needed.");
}

const presetSelect = el<HTMLSelectElement>("preset");
for (const preset of PRESETS) {
  const option = document.createElement("option");
  option.value = preset.target;
  option.textContent = preset.label;
  presetSelect.appendChild(option);
}
presetSelect.addEventListener("change", () => {
  const chosen = presetSelect.value;
  presetSelect.selectedIndex = 0;
  if (!chosen) return;
  if (chosen === "sample") {
    loadSample();
    return;
  }
  el<HTMLInputElement>("target").value = chosen;
  el<HTMLFormElement>("load-form").requestSubmit();
});

/**
 * Fetch a repository and index it.
 *
 * Throws rather than reporting: the caller decides what a failure means. Pressing
 * Load and having it fail is worth an error in the status bar; failing to reach
 * GitHub on the very first page load is worth standing something else up instead.
 */
async function loadTarget(spec: string) {
  const target = parseTarget(spec);
  if (!target) throw new Error("Give an owner/repo, or a github.com URL.");

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
  } finally {
    button.disabled = false;
  }
}

el<HTMLFormElement>("load-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  try {
    await loadTarget(el<HTMLInputElement>("target").value);
  } catch (error) {
    say(String(error instanceof Error ? error.message : error), "error");
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

// ----------------------------------------------------------- the top bar icons

function buildNavIcons() {
  const bar = el<HTMLElement>("nav-icons");
  bar.innerHTML = "";

  const add = (id: string, icon: string, label: string, run: () => void) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "icon-button";
    button.id = id;
    decorate(button, icon, label);
    button.addEventListener("click", run);
    bar.appendChild(button);
    return button;
  };

  add("nav-back", "back", "Back (Alt+←)", goBack).disabled = true;
  add("nav-forward", "forward", "Forward (Alt+→)", goForward).disabled = true;

  for (const action of ACTIONS.filter((a) => a.icon)) {
    const label =
      action.id === "definition" ? `${action.label} (F12)` : action.label;
    const button = add(`nav-${action.id}`, action.icon!, label, () => {
      if (action.id === "definition") goToDefinition();
      else void run(action);
    });
    button.dataset.act = action.id;
  }
}

// --------------------------------------------------------- the toolbar menus

/**
 * What the menu is about, in the words the subject deserves.
 *
 * A refactoring acts on a selection when there is one and on the symbol under the
 * cursor otherwise, and the two are not interchangeable — Extract needs a range,
 * Rename needs a name. Saying which is in force is the difference between a menu you
 * trust and one you try twice.
 */
function subjectLine(): string {
  if (!workspace || !current) return `<span class="dim">no file open</span>`;
  const range = selectionRange();
  if (range) {
    return `<strong>${escapeHtml(range.replace("-", " → "))}</strong> ` +
      `<span class="dim">selected</span>`;
  }
  const position = editor.getPosition();
  if (!position) return `<span class="dim">put the cursor somewhere</span>`;
  try {
    const here = JSON.parse(workspace.at(current, position.lineNumber, position.column));
    if (here.name) {
      return `<strong>${escapeHtml(here.name)}</strong> ` +
        `<span class="dim">${escapeHtml(here.kind ?? "")}</span>`;
    }
  } catch {
    // Fall through to the honest answer below.
  }
  return `<span class="dim">nothing the index knows here</span>`;
}

function itemsFor(groups: readonly string[]) {
  return ACTIONS.filter((a) => groups.includes(a.group)).map((action) => ({
    id: action.id,
    label: action.label,
    group: action.group,
    mutates: action.mutates,
    disabled: unavailable(action),
  }));
}

function pick(id: string) {
  const action = ACTIONS.find((a) => a.id === id);
  if (!action) return;
  if (action.id === "definition") goToDefinition();
  else void run(action);
}

/**
 * Drop a menu under its button, and let a second click close it again.
 *
 * The open state has to be sampled at `pointerdown`, because the menu closes itself
 * on any pointer press outside it — which includes this button. By the time `click`
 * runs the menu is already shut, so asking then would reopen it every time and the
 * button would never appear to toggle.
 */
function dropdown(button: HTMLElement, groups: readonly string[]) {
  let wasOpen = false;
  button.addEventListener("pointerdown", () => {
    wasOpen = menu.isOpen() && menu.openedBy() === button;
  });
  button.addEventListener("click", () => {
    if (wasOpen) {
      menu.close();
      return;
    }
    const box = button.getBoundingClientRect();
    menu.open(box.left, box.bottom + 5, subjectLine(), itemsFor(groups), pick, button);
  });
}

function buildToolbarMenus() {
  dropdown(el<HTMLButtonElement>("analyse-menu"), ["Analyse"]);
  dropdown(el<HTMLButtonElement>("refactor-menu"), [
    "Rename and move",
    "Extract and inline",
    "Rewrite",
  ]);
}

// ------------------------------------------------- rewrite as another language

/**
 * Offer the languages this file could be written as, and say why the rest are not.
 *
 * The refusals are the useful half. "You cannot turn Rust into Python, because that
 * is a translation and this tool parses syntax" is a more valuable thing for a menu
 * to say than an empty list, and an empty list is what a shorter menu would be.
 */
function openTranslateMenu(anchor: HTMLElement) {
  if (!workspace || !current) {
    show(`<p class="hint">Open a file first.</p>`, "Rewrite as");
    return;
  }
  let options: any[] = [];
  try {
    options = JSON.parse(workspace.translations(current));
  } catch (e) {
    show(`<p class="err">${escapeHtml(String(e))}</p>`, "Rewrite as");
    return;
  }
  if (!Array.isArray(options)) {
    show(`<p class="err">${escapeHtml(String((options as any).error))}</p>`, "Rewrite as");
    return;
  }

  const possible = options.filter((o) => !o.unavailable);
  const box = anchor.getBoundingClientRect();
  menu.open(
    box.left,
    box.bottom + 5,
    `<strong>${escapeHtml(current.split("/").pop() ?? current)}</strong> ` +
      `<span class="dim">as ${possible.length ? "…" : "— nothing, see below"}</span>`,
    options.map((o) => ({
      id: o.language,
      label: o.destination
        ? `${o.language} → ${o.destination.split("/").pop()}` +
          (o.framework && o.draft ? ` (${o.draft.split(";")[0]})` : "")
        : o.language,
      // Four groups, because these are four different promises and must not sit
      // together: the same bytes under another grammar, a draft a person has to
      // finish, a port to another framework, and a refusal with its reason.
      group: o.unavailable
        ? "Not possible"
        : o.framework
          ? "Port to a framework"
          : o.draft
            ? "Translate (a draft)"
            : "Write it as",
      mutates: Boolean(o.draft),
      disabled: o.unavailable ?? null,
    })),
    (language) => {
      const chosen = options.find((o) => o.language === language);
      if (chosen?.unavailable) return;
      const applied = JSON.parse(workspace!.translate(current, language));
      if (applied.error) {
        show(
          `<p class="err">${escapeHtml(applied.error)}</p>` +
            `<p class="hint">Nothing was written.</p>`,
          "Rewrite as",
        );
        return;
      }
      syncFromWorkspace(applied.files);
      show(render(applied, current), `Rewrite as ${language}`);
      // The new file is the point; open it.
      const written = applied.files[0]?.path;
      if (written) openFile(written);
    },
    anchor,
  );
}

// --------------------------------------------------------- the context menu

/** Everything that applies to what is under the cursor, with reasons for the rest. */
function openContextMenu(x: number, y: number) {
  if (!workspace || !current) return;
  const subject = subjectLine();
  menu.open(x, y, subject, itemsFor(GROUPS), pick);
}

el<HTMLElement>("editor").addEventListener("contextmenu", (e) => {
  e.preventDefault();
  openContextMenu(e.clientX, e.clientY);
});

// The same menu from the keyboard, and from the outline, so nothing is mouse-only.
addEventListener("keydown", (e) => {
  if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
    e.preventDefault();
    const box = el<HTMLElement>("editor").getBoundingClientRect();
    openContextMenu(box.left + 60, box.top + 60);
  }
});

// --------------------------------------------------------- editor keybindings

// What an editor does: F12 and ⌘/Ctrl-click go to the definition, Alt+arrows walk
// the jumps you made. Bound through Monaco so they work while it has focus.
editor.addCommand(monaco.KeyCode.F12, goToDefinition);
editor.addCommand(monaco.KeyMod.Alt | monaco.KeyCode.LeftArrow, goBack);
editor.addCommand(monaco.KeyMod.Alt | monaco.KeyCode.RightArrow, goForward);

editor.onMouseDown((e) => {
  const modified = e.event.ctrlKey || e.event.metaKey;
  if (!modified || !e.target.position) return;
  e.event.preventDefault();
  editor.setPosition(e.target.position);
  goToDefinition();
});

// --------------------------------------------------------------------- theme

/** Monaco keeps its own themes and does not read CSS variables. */
function syncEditorTheme() {
  monaco.editor.setTheme(currentTheme() === "dark" ? "vs-dark" : "vs");
}

function buildThemeButton() {
  const button = el<HTMLButtonElement>("theme-button");
  const paint = () => {
    const dark = currentTheme() === "dark";
    // The icon shows what a click gives you, not what you already have.
    decorate(button, dark ? "sun" : "moon", dark ? "Light theme" : "Dark theme");
  };
  button.addEventListener("click", toggleTheme);
  onThemeChange(() => {
    paint();
    syncEditorTheme();
  });
  paint();
}

// ----------------------------------------------------------------------- start

installTheme();
syncEditorTheme();
buildThemeButton();

const translateButton = el<HTMLButtonElement>("translate-button");
decorate(translateButton, "translate", "Rewrite this file as another language");
{
  // Same toggle discipline as the toolbar menus: the open state is sampled at
  // pointerdown, because the menu closes on any press outside itself.
  let wasOpen = false;
  translateButton.addEventListener("pointerdown", () => {
    wasOpen = menu.isOpen() && menu.openedBy() === translateButton;
  });
  translateButton.addEventListener("click", () => {
    if (wasOpen) {
      menu.close();
      return;
    }
    openTranslateMenu(translateButton);
  });
}

const helpButton = el<HTMLButtonElement>("help-button");
helpButton.classList.add("round");
decorate(helpButton, "help", "Help and workspace overview");
helpButton.addEventListener("click", () => openHelp());
installHelp(
  [
    ...PAGES,
    // The live pages ask the workspace when opened, so they are never stale.
    ...livePages((method) => {
      if (!workspace) return null;
      const call = (workspace as unknown as Record<string, () => string>)[method];
      return typeof call === "function" ? call.call(workspace) : null;
    }),
  ],
  (target) => jumpTo(parsePlace(target)),
);

installShell();
setResizeHandler(() => editor.layout());
buildNavIcons();
buildToolbarMenus();
say("Loading the analysis…", "busy");
init({ module_or_path: wasmUrl })
  .then(async () => {
    el<HTMLButtonElement>("load").disabled = false;
    // Start with something real loaded. A page whose every button is greyed out until
    // you have thought of a repository is a page nobody tries. If GitHub will not
    // answer — rate limits are 60 requests an hour for an anonymous browser — the
    // bundled sample stands in, and says so rather than pretending it was the plan.
    // `?repo=` picks the workspace here exactly as it does for the JSON renderings —
    // one parameter meaning two different things depending on the mode would be a
    // trap for anyone sharing a link.
    const asked = new URLSearchParams(location.search).get("repo");
    if (asked === "sample") {
      loadSample();
      return;
    }
    const wanted = asked ?? DEFAULT_REPOSITORY;
    el<HTMLInputElement>("target").value = wanted;
    try {
      await loadTarget(wanted);
    } catch (error) {
      loadSample();
      say(
        `${error instanceof Error ? error.message : error} — showing the bundled sample instead.`,
        "error",
      );
    }
  })
  .catch((e) => say(`The analysis failed to load: ${e}`, "error"));
