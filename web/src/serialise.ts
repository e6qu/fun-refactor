/**
 * The playground, for something that is not a person.
 *
 * Two renderings, chosen by `?render_as=`:
 *
 *   json       the view a person would see, described instead of drawn — the panes,
 *              what is in them, the toolbar, and which actions are available on the
 *              current subject and why the rest are not.
 *   json_data  the analysis itself, with no view: what was indexed, what is defined
 *              where, what is dead, what is duplicated.
 *
 * The split matters. `json` answers "what would I be looking at"; `json_data` answers
 * "what does the tool know". A caller that wants to check the interface offers Extract
 * on a selection needs the first; a caller that wants the dead code needs the second,
 * and should not have to read a description of a sidebar to get it.
 *
 * Both accept `?repo=` and `?file=` — a rendering of one fixed repository would be a
 * demonstration instead of a tool.
 */

import { ACTIONS, GROUPS } from "./actions";
import { loadRepository, parseTarget } from "./github";
import type { Mode } from "./main";
import init, { Workspace } from "./wasm/fun_refactor.js";
import wasmUrl from "./wasm/fun_refactor_bg.wasm?url";

const SAMPLE = import.meta.glob("../sample/**/*", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const DEFAULT_REPOSITORY = "psf/requests/tree/main/src/requests";

/** Parse without throwing, so one unhappy answer cannot lose the whole document. */
function ask<T>(json: string, fallback: T): T {
  try {
    const value = JSON.parse(json);
    return value && typeof value === "object" && "error" in value ? fallback : value;
  } catch {
    return fallback;
  }
}

function sample(): Record<string, string> {
  const files: Record<string, string> = {};
  for (const [key, text] of Object.entries(SAMPLE)) {
    files[key.replace("../sample/", "")] = text;
  }
  return files;
}

/**
 * What a person would be looking at, as data.
 *
 * Written against the same `ACTIONS` list and the same availability rule the page
 * uses, so this cannot describe a menu the page would not draw. What it does not
 * reproduce is pixels: pane sizes, themes and scroll positions are not part of what
 * the interface *offers*.
 */
function view(workspace: Workspace, name: string, open: string) {
  const files = ask<any[]>(workspace.files(), []);
  const stats = ask<any>(workspace.stats(), {});
  const symbols = open ? ask<any[]>(workspace.symbols(open), []) : [];
  const language = files.find((f) => f.path === open)?.language ?? null;

  // Nothing is selected and no cursor has been placed, so the subject is the file.
  // Every action that needs a position or a selection says so, with its reason.
  const availability = (needs: string) => {
    if (needs === "workspace") return null;
    if (!open) return "Open a file first.";
    if (needs === "file") return null;
    if (needs === "selection") return "Select the code to lift out first.";
    return "Put the cursor on a name first.";
  };

  const actionsIn = (groups: readonly string[]) =>
    ACTIONS.filter((a) => groups.includes(a.group)).map((a) => ({
      id: a.id,
      label: a.label,
      group: a.group,
      describes: a.describes,
      edits: Boolean(a.mutates),
      needs: a.needs,
      asks: a.ask ? { label: a.ask.label, example: a.ask.example } : null,
      unavailable: availability(a.needs),
    }));

  return {
    workspace: { name, files: stats.files ?? files.length },
    toolbar: {
      navigate: ACTIONS.filter((a) => a.icon).map((a) => ({
        id: a.id,
        label: a.label,
        describes: a.describes,
        unavailable: availability(a.needs),
      })),
      menus: [
        { label: "Analyse", items: actionsIn(["Analyse"]) },
        {
          label: "Refactor",
          items: actionsIn(["Rename and move", "Extract and inline", "Rewrite"]),
        },
      ],
      // The same list the right-click menu builds, in the same order.
      contextMenu: actionsIn(GROUPS),
      help: { pages: ["Getting around", "What this is", "This workspace", "Entry points", "What it can do"] },
    },
    panes: [
      {
        id: "files",
        title: "Files",
        items: files.map((f) => ({
          path: f.path,
          language: f.language,
          indexed: f.indexed,
          open: f.path === open,
        })),
      },
      {
        id: "outline",
        title: "Outline",
        of: open,
        items: symbols.map((s) => ({
          name: s.name,
          kind: s.kind,
          line: s.line,
          col: s.col,
          exported: s.exported,
        })),
      },
      {
        id: "structure",
        title: "Structure",
        of: open,
        // The tree itself belongs to json_data; here it is the pane's shape.
        nodes: open ? countNodes(ask<any>(workspace.ast(open), { children: [] })) : 0,
      },
      { id: "result", title: "Result", content: null },
    ],
    editor: {
      open,
      language,
      // No cursor has been placed, so there is no subject and nothing to say about it.
      cursor: null,
      subject: null,
    },
  };
}

function countNodes(node: any): number {
  if (!node || !Array.isArray(node.children)) return 0;
  return 1 + node.children.reduce((n: number, c: any) => n + countNodes(c), 0);
}

/** What the tool knows, with no interface wrapped around it. */
function data(workspace: Workspace, name: string, open: string) {
  return {
    workspace: { name },
    stats: ask<any>(workspace.stats(), null),
    files: ask<any[]>(workspace.files(), []),
    capabilities: ask<any[]>(workspace.capabilities(), []),
    entrypoints: ask<any[]>(workspace.entrypoints(), []),
    graph: ask<any>(workspace.graph(), null),
    unused: ask<any[]>(workspace.unused(), []),
    duplicates: ask<any[]>(workspace.duplicates(40), []),
    stitch: ask<any>(workspace.stitch(), null),
    open: open
      ? {
          path: open,
          text: workspace.read(open),
          symbols: ask<any[]>(workspace.symbols(open), []),
          ast: ask<any>(workspace.ast(open), null),
        }
      : null,
  };
}

/** Put the document in one state or the other, and say which. */
function print(payload: unknown) {
  const text = JSON.stringify(payload, null, 2);
  document.head.innerHTML = "<title>fun-refactor</title>";
  document.body.textContent = text;
  document.body.style.cssText =
    "font:12px ui-monospace,monospace;white-space:pre;margin:0;padding:1rem";
  // A caller driving a browser reads this instead of scraping the page.
  (window as unknown as Record<string, unknown>).__fr = payload;
  document.documentElement.dataset.frReady = "true";
}

export async function emit(mode: Mode) {
  const parameters = new URLSearchParams(location.search);
  const requested = parameters.get("repo") ?? DEFAULT_REPOSITORY;

  try {
    await init({ module_or_path: wasmUrl });

    let files: Record<string, string>;
    let name: string;
    if (requested === "sample") {
      files = sample();
      name = "bundled sample";
    } else {
      const target = parseTarget(requested);
      if (!target) throw new Error(`'${requested}' is not an owner/repo or a github.com URL`);
      const loaded = await loadRepository(target);
      files = loaded.files;
      name = `${target.owner}/${target.repo}@${loaded.ref}`;
    }

    const workspace = new Workspace(files);
    const listed = ask<any[]>(workspace.files(), []);

    // Which file is "open" — the one the panes describe. Named, or the largest that
    // was indexed, which is what the page itself opens.
    const wanted = parameters.get("file");
    if (wanted && files[wanted] === undefined) {
      throw new Error(`'${wanted}' is not in this workspace`);
    }
    const open =
      wanted ??
      listed
        .filter((f) => f.indexed)
        .sort((a, b) => (files[b.path]?.length ?? 0) - (files[a.path]?.length ?? 0))[0]?.path ??
      "";

    // Named one by one and not "json or else data": a mode added to the
    // dispatcher and forgotten here would otherwise be answered with the wrong
    // rendering and no complaint. `web/test/render_modes.mjs` checks each one
    // appears.
    let body: Record<string, unknown>;
    switch (mode) {
      case "json":
        body = view(workspace, name, open);
        break;
      case "json_data":
        body = data(workspace, name, open);
        break;
      default:
        throw new Error(`'${mode}' has no rendering here`);
    }
    print({ render_as: mode, ...body });
  } catch (error) {
    print({
      render_as: mode,
      error: error instanceof Error ? error.message : String(error),
      repo: requested,
    });
    document.documentElement.dataset.frError = "true";
  }
}
