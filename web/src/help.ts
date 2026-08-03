/**
 * The help modal: what this is, how to point at things, and what the workspace
 * currently contains.
 *
 * The four "understand the workspace" answers — index stats, the capability matrix,
 * the entry points, the file outline — used to be four buttons competing with
 * twenty-eight others. They are not actions on a symbol; they are things you read
 * once to know where you are, so they live here, one to a page, over everything else.
 *
 * The live pages ask the wasm when opened rather than when built: a workspace can be
 * replaced, and a stale answer under a help tab is a quiet lie.
 */

import { render, escapeHtml } from "./render";

export interface HelpPage {
  id: string;
  label: string;
  /** Static prose, or a function that asks the workspace when the tab is opened. */
  body: () => string;
}

const KEYS = `
<h3>Pointing at things</h3>
<p>Everything acts on <em>what the cursor is on</em>. The status bar under the editor
   names it, and shows the coordinate — <code>path:line:col</code> — which is exactly
   what <code>fr</code> takes as a target on the command line. Click it to copy.</p>
<table>
  <tr><th>Do this</th><th>Get that</th></tr>
  <tr><td>Click a name</td><td>The status bar names the symbol, its kind, and where it is defined</td></tr>
  <tr><td><code>⌘</code>/<code>Ctrl</code> + click, or <code>F12</code></td><td>Jump to the definition</td></tr>
  <tr><td>Right-click</td><td>Every action that applies to it</td></tr>
  <tr><td>Alt + ←&nbsp;/&nbsp;→</td><td>Back and forward through the jumps you made</td></tr>
  <tr><td>Click a node in <strong>Structure</strong></td><td>Select exactly the bytes it covers — how you pick a range for Extract</td></tr>
  <tr><td>Select code, then Extract</td><td>Acts on the selection rather than the cursor</td></tr>
</table>

<h3>What the tiers mean</h3>
<p>Every reference carries the rule that found it. Only the top two are ever
   rewritten; the rest are handed back as a list for you to judge.</p>
<table>
  <tr><td><span class="tier exact">exact</span></td><td>Resolved by lexical scope or an unambiguous definition. Safe to edit.</td></tr>
  <tr><td><span class="tier import-qualified">import-qualified</span></td><td>Resolved through an import binding. Safe to edit.</td></tr>
  <tr><td><span class="tier field-based">field-based</span></td><td>A member access whose receiver type is not known. Reported, never rewritten.</td></tr>
  <tr><td><span class="tier name-only">name-only</span></td><td>The name matched and nothing else did. Reported, never rewritten.</td></tr>
</table>
<p>A refusal is an answer. When the tool declines, it names the rule that stopped it
   and changes nothing — that is the feature, not an error.</p>
`;

const ABOUT = `
<h3>This runs in your browser</h3>
<p>The whole analysis is compiled to WebAssembly. There is no server: a public
   repository is fetched from GitHub into this tab, indexed here, and every answer and
   every edit happens here. Nothing is uploaded, and nothing on GitHub changes — the
   diff is the artifact, and <strong>Download patch</strong> hands you a file
   <code>git apply</code> will take.</p>
<h3>What it is doing</h3>
<p>Each file is parsed by tree-sitter into a tree that keeps every byte, and a set of
   facts is extracted from it — symbols, references, scopes, imports — each carrying a
   byte span. The trees are not kept; the facts are. An edit is a byte-range splice,
   so everything outside the range is untouched by construction, and every changed
   file is reparsed before the edit is allowed to stand.</p>
<h3>Where it is weaker than a compiler</h3>
<p>It resolves what the syntax proves and says so when it cannot. A value held in a
   map and called through it, a class named only in a string, a Helm value passed on a
   command line — these are undecidable from the source, and the tool reports them
   rather than guessing. That is why so much comes back as a list to review.</p>
<h3>Fifteen languages, one index</h3>
<p>Rust, Go, Zig, TypeScript, TSX, Python, Bash, HTML, CSS, SCSS, HCL, YAML, Helm,
   XML and Markdown all land in the same index, which is what lets a rename cross from
   a chart value into the code that reads it.</p>
`;

export const PAGES: HelpPage[] = [
  { id: "keys", label: "Getting around", body: () => KEYS },
  { id: "about", label: "What this is", body: () => ABOUT },
];

/** The pages that ask the workspace. Registered by the page, which owns it. */
export function livePages(ask: (method: string) => string | null): HelpPage[] {
  const live = (method: string, empty: string) => () => {
    const json = ask(method);
    if (json === null) {
      return `<p class="hint">Load a workspace first.</p>`;
    }
    try {
      const value = JSON.parse(json);
      if (value && typeof value === "object" && "error" in value) {
        return `<p class="err">${escapeHtml(String(value.error))}</p>`;
      }
      return `<div class="help-live">${render(value, "", empty)}</div>`;
    } catch (e) {
      return `<p class="err">${escapeHtml(String(e))}</p>`;
    }
  };

  return [
    {
      id: "stats",
      label: "This workspace",
      body: () =>
        `<h3>What was indexed</h3>` +
        live("stats", "Nothing is indexed.")() +
        `<p class="hint">A file in a language this build has no grammar for is left out
          and reported, never silently dropped.</p>`,
    },
    {
      id: "entrypoints",
      label: "Entry points",
      body: () =>
        `<h3>Where execution can start</h3>` +
        `<p>These anchor the dead-code report: a symbol is dead when nothing reachable
           from one of these reaches it. Mains, HTTP handlers, tests and probes are
           recognised from built-in catalogs.</p>` +
        live("entrypoints", "No entry points at all — everything unexported will read as dead.")(),
    },
    {
      id: "capabilities",
      label: "What it can do",
      body: () =>
        `<h3>Capability by language</h3>` +
        `<p>Computed by asking each refactoring's own predicate, so this cannot drift
           from what the code actually supports.</p>` +
        live("capabilities", "This build reports no capabilities, which should be impossible.")(),
    },
  ];
}

let dialog: HTMLDialogElement | null = null;
let pages: HelpPage[] = [];
let active = "keys";

/** Build the tab strip once. Rebuilding it on every click would throw away the
 *  focus of whoever was using the keyboard to get there. */
function buildTabs() {
  if (!dialog) return;
  const tabs = dialog.querySelector(".help-tabs")!;
  tabs.innerHTML = pages
    .map(
      (p) =>
        `<button type="button" data-page="${escapeHtml(p.id)}" role="tab" ` +
        `aria-selected="false">${escapeHtml(p.label)}</button>`,
    )
    .join("");
}

/** Show the selected page. Only the selection and the body change. */
function draw() {
  if (!dialog) return;
  for (const tab of dialog.querySelectorAll<HTMLElement>("[data-page]")) {
    tab.setAttribute("aria-selected", String(tab.dataset.page === active));
  }
  const body = dialog.querySelector(".help-body")!;
  const page = pages.find((p) => p.id === active) ?? pages[0];
  body.innerHTML = page ? page.body() : "";
  body.scrollTop = 0;
}

export function installHelp(all: HelpPage[], onGoto: (target: string) => void) {
  pages = all;
  dialog = document.getElementById("help") as HTMLDialogElement;
  buildTabs();

  dialog.addEventListener("click", (e) => {
    const tab = (e.target as HTMLElement).closest("[data-page]");
    if (tab) {
      active = (tab as HTMLElement).dataset.page!;
      draw();
      return;
    }
    // A location inside a help page is still a location worth going to; going there
    // means leaving the modal, so it closes.
    const link = (e.target as HTMLElement).closest("[data-goto]");
    if (link) {
      dialog!.close();
      onGoto((link as HTMLElement).dataset.goto!);
    }
  });
}

export function openHelp(page?: string) {
  if (!dialog) return;
  if (page) active = page;
  draw();
  if (!dialog.open) dialog.showModal();
}
