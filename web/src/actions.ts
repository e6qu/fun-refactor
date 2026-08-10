/**
 * What the playground can do, as data.
 *
 * Every capability the wasm exposes appears here exactly once, with what it needs
 * before it can run — a cursor position, a selection, a name to type — and how to
 * render what comes back. Keeping it as a list and not as a wall of buttons is
 * what makes "can I try every feature from the site?" answerable: the page is built
 * from this array, so a capability that exists in the API and not on the page is a
 * missing entry instead of a forgotten button.
 */

import type { Workspace } from "./wasm/fun_refactor.js";

/** What the action needs before it can be offered. */
export type Needs = "workspace" | "file" | "position" | "selection";

export interface Ask {
  /** Field label. */
  label: string;
  /** Placeholder, and the shape the value has to take. */
  example: string;
  /** Why it is being asked, in one line. */
  why?: string;
  /** Prefill from the identifier under the cursor. */
  fromSubject?: boolean;
}

export interface Context {
  path: string;
  line: number;
  col: number;
  /** `line:col-line:col`, present when there is a selection. */
  range: string | null;
  /** The identifier under the cursor, when there is one. */
  subject: string;
  /** What the ask collected, if the action asked for anything. */
  answer: string;
  /** The open file's language, as the analysis names it. */
  language: string;
}

export interface Action {
  id: string;
  label: string;
  group: string;
  needs: Needs;
  /** One line under the button. */
  describes: string;
  /**
   * What to say when the answer is an empty list.
   *
   * An empty array has no shape to read, so without this every question that finds
   * nothing gives the same shrug — and "no references resolve to this" and "this
   * interface has no implementations" are different facts about the code.
   */
  empty?: string;
  ask?: Ask;
  /**
   * An icon name, for the actions that also live in the top bar.
   *
   * Only navigation gets one: it is what you do most, and a trip through a list of
   * thirty buttons for "where is this defined" is the reason the panel felt like a
   * form instead of an editor.
   */
  icon?: string;
  /** Does this rewrite the workspace? Mutations refresh the editor and the file list. */
  mutates?: boolean;
  run: (w: Workspace, c: Context) => string;
}

export const GROUPS = [
  "Navigate",
  "Analyse",
  "Rename and move",
  "Extract and inline",
  "Rewrite",
] as const;

export const ACTIONS: Action[] = [
  // --------------------------------------------------------------- Navigate
  {
    id: "definition",
    label: "Go to definition",
    group: "Navigate",
    icon: "definition",
    needs: "position",
    describes: "Every definition, not just the first — an abstraction has several",
    run: (w, c) => w.definition(c.path, c.line, c.col),
  },
  {
    id: "references",
    label: "Find references",
    group: "Navigate",
    icon: "references",
    needs: "position",
    describes: "Uses of this symbol, each tagged with how certain the resolution is",
    empty: "No references resolve to that symbol.",
    run: (w, c) => w.references(c.path, c.line, c.col),
  },
  {
    id: "usages",
    label: "Usages, with the near misses",
    group: "Navigate",
    icon: "usages",
    needs: "position",
    describes: "Uses, plus same-named occurrences that are not uses of this one",
    run: (w, c) => w.usages(c.path, c.line, c.col),
  },
  {
    id: "implementations",
    label: "Implementations",
    group: "Navigate",
    icon: "implementations",
    needs: "position",
    describes: "The concrete types or methods behind an interface, trait or base class",
    empty: "Nothing implements that — it is a concrete declaration, or no type in this workspace answers to it.",
    run: (w, c) => w.implementations(c.path, c.line, c.col),
  },

  // ---------------------------------------------------------------- Analyse
  {
    id: "callers",
    label: "Callers",
    group: "Analyse",
    needs: "position",
    describes: "Who reaches this, up to four levels out",
    run: (w, c) => w.callers(c.path, c.line, c.col, 4),
  },
  {
    id: "callees",
    label: "Callees",
    group: "Analyse",
    needs: "position",
    describes: "What this reaches, up to four levels down",
    run: (w, c) => w.callees(c.path, c.line, c.col, 4),
  },
  {
    id: "graph",
    label: "Call graph size",
    group: "Analyse",
    needs: "workspace",
    describes: "Functions, edges, and how many came from dispatch instead of a name",
    run: (w) => w.graph(),
  },
  {
    id: "impact",
    label: "Impact of changing this",
    group: "Analyse",
    needs: "position",
    describes: "What a change here would reach, and how far",
    run: (w, c) => w.impact(c.path, c.line, c.col),
  },
  {
    id: "flow_back",
    label: "Where this value came from",
    group: "Analyse",
    needs: "position",
    describes: "Backwards through assignments and parameters to a source",
    run: (w, c) => w.flow_back(c.path, c.line, c.col),
  },
  {
    id: "flow_forward",
    label: "Where this value goes",
    group: "Analyse",
    needs: "position",
    describes: "Forwards to the places it is finally used",
    run: (w, c) => w.flow_forward(c.path, c.line, c.col),
  },
  {
    id: "stitch",
    label: "Config to code",
    group: "Analyse",
    needs: "workspace",
    describes: "Where a chart value, a Terraform variable or an env name meets the code",
    empty: "Nothing in the configuration meets the code by a name this can follow.",
    run: (w) => w.stitch(),
  },
  {
    id: "unused",
    label: "Dead code",
    group: "Analyse",
    needs: "workspace",
    describes: "Nothing reaches these from any entry point or export",
    empty: "Everything here is reachable from an entry point or an export.",
    run: (w) => w.unused(),
  },
  {
    id: "duplicates",
    label: "Copy-paste",
    group: "Analyse",
    needs: "workspace",
    describes: "Blocks of 40 tokens or more that appear more than once",
    empty: "No block of 40 tokens or more appears twice.",
    run: (w) => w.duplicates(40),
  },

  // -------------------------------------------------------- Rename and move
  {
    id: "rename",
    label: "Rename",
    group: "Rename and move",
    needs: "position",
    mutates: true,
    describes: "Every certain use, across every language that names it",
    ask: {
      label: "New name",
      example: "check_reading",
      why: "Uses the resolver is not certain about are reported, not rewritten.",
      fromSubject: true,
    },
    run: (w, c) => w.rename(c.path, c.line, c.col, c.answer),
  },
  {
    id: "move_symbol",
    label: "Move to another file",
    group: "Rename and move",
    needs: "position",
    mutates: true,
    describes: "Carries the imports it needs and fixes the ones it leaves behind",
    ask: {
      label: "Destination path",
      example: "src/convert.rs",
      why: "Relative to the workspace root, the same paths the file list shows.",
    },
    run: (w, c) => w.move_symbol(c.path, c.line, c.col, c.answer),
  },
  {
    id: "delete",
    label: "Safe delete",
    group: "Rename and move",
    needs: "position",
    mutates: true,
    describes: "Refuses while anything still uses it",
    run: (w, c) => w.delete(c.path, c.line, c.col),
  },
  {
    id: "organize_imports",
    label: "Organize imports",
    group: "Rename and move",
    needs: "file",
    mutates: true,
    describes: "Drop what is unused and sort what is left",
    run: (w, c) => w.organize_imports(c.path),
  },
  {
    id: "remove_flag",
    label: "Retire a feature flag",
    group: "Rename and move",
    needs: "workspace",
    mutates: true,
    describes: "Assume it was always on, and delete the branches that assumed otherwise",
    ask: {
      label: "Flag name",
      example: "REPORT_IN_CELSIUS",
      why: "A constant, variable or function whose value decided a branch.",
      fromSubject: true,
    },
    run: (w, c) => w.remove_flag(c.answer, true),
  },

  // ---------------------------------------------------- Extract and inline
  {
    id: "extract_variable",
    label: "Extract variable",
    group: "Extract and inline",
    needs: "selection",
    mutates: true,
    describes: "Name the selected expression and bind it above",
    ask: { label: "Name for it", example: "scaled" },
    run: (w, c) => w.extract_variable(c.path, c.range!, c.answer),
  },
  {
    id: "extract_function",
    label: "Extract function",
    group: "Extract and inline",
    needs: "selection",
    mutates: true,
    describes: "Lift the selected statements out, working out the parameters",
    ask: { label: "Name for it", example: "accumulate" },
    run: (w, c) => w.extract_function(c.path, c.range!, c.answer),
  },
  {
    id: "inline_variable",
    label: "Inline variable",
    group: "Extract and inline",
    needs: "position",
    mutates: true,
    describes: "Replace each use with the value, and drop the binding",
    run: (w, c) => w.inline_variable(c.path, c.line, c.col),
  },
  {
    id: "inline_call",
    label: "Inline call",
    group: "Extract and inline",
    needs: "position",
    mutates: true,
    describes: "Put the callee's body at the call site",
    run: (w, c) => w.inline_call(c.path, c.line, c.col),
  },
  {
    id: "signature",
    label: "Change signature",
    group: "Extract and inline",
    needs: "position",
    mutates: true,
    describes: "Add, remove or reorder a parameter, and every call site with it",
    ask: {
      label: "Change",
      example: "add:1:offset: f64:0.0",
      why: "One of remove:<i>, move:<from>:<to>, add:<i>:<declaration>:<argument>.",
    },
    run: (w, c) => w.signature(c.path, c.line, c.col, c.answer),
  },

  // ---------------------------------------------------------------- Rewrite
  {
    id: "rewrites_at",
    label: "What applies here?",
    group: "Rewrite",
    needs: "position",
    describes: "The local transformations available at the cursor",
    empty: "No local transformation applies at the cursor.",
    run: (w, c) => w.rewrites_at(c.path, c.line, c.col),
  },
  {
    id: "invert-if",
    label: "Invert if",
    group: "Rewrite",
    needs: "position",
    mutates: true,
    describes: "Swap the branches and negate the condition",
    run: (w, c) => w.rewrite(c.path, c.line, c.col, "invert-if"),
  },
  {
    id: "guard-clause",
    label: "Guard clause",
    group: "Rewrite",
    needs: "position",
    mutates: true,
    describes: "Turn a wrapping if into an early return",
    run: (w, c) => w.rewrite(c.path, c.line, c.col, "guard-clause"),
  },
  {
    id: "de-morgan",
    label: "De Morgan",
    group: "Rewrite",
    needs: "position",
    mutates: true,
    describes: "Push a negation through an and/or, keeping the grouping",
    run: (w, c) => w.rewrite(c.path, c.line, c.col, "de-morgan"),
  },
  {
    id: "restructure",
    label: "Rewrite a pattern everywhere",
    group: "Rewrite",
    needs: "file",
    mutates: true,
    describes: "A syntactic search and replace, matched against the tree",
    ask: {
      label: "Pattern → template",
      example: "len($x) == 0 => not $x",
      why: "`$name` captures a subtree. The language is taken from the open file.",
    },
    run: (w, c) => {
      const [pattern, template] = c.answer.split("=>").map((s) => s.trim());
      if (!pattern || !template) {
        return JSON.stringify({
          error: "write it as `pattern => template`, with one arrow",
        });
      }
      return w.restructure(c.language, pattern, template);
    },
  },
];
