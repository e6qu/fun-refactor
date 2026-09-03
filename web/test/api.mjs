/**
 * Drives every method the browser API exposes against the bundled sample workspace.
 *
 * The playground is the only place this code runs as WebAssembly, and a browser is a
 * bad place to find out that a method traps. This is the same wasm the site ships,
 * loaded in Node, called once per capability per language that has one. It asserts
 * shapes and not exact numbers: the point is that nothing throws, nothing returns
 * an unexpected `error`, and each answer is the kind of thing the view can render.
 *
 *     node web/test/api.mjs
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

// wasm-bindgen's `--target web` output calls `fetch` on a URL. Node has one, but a
// file:// URL is not fetchable, so the bytes are read and handed over directly.
const { default: init, Workspace, version } = await import(
  join(root, "src/wasm/fun_refactor.js")
);
await init({
  module_or_path: readFileSync(join(root, "src/wasm/fun_refactor_bg.wasm")),
});

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) out.push(...walk(path));
    else out.push(path);
  }
  return out;
}

const sampleRoot = join(root, "sample");
const files = {};
for (const path of walk(sampleRoot)) {
  files[relative(sampleRoot, path)] = readFileSync(path, "utf8");
}

let failures = 0;
let checks = 0;

function check(name, fn) {
  checks += 1;
  try {
    const note = fn();
    console.log(`  ok   ${name}${note ? ` — ${note}` : ""}`);
  } catch (e) {
    failures += 1;
    console.log(`  FAIL ${name}: ${e.message}`);
  }
}

/** Parse a wasm answer, failing loudly if it refused. */
function json(text, { allowError = false } = {}) {
  const value = JSON.parse(text);
  if (value && typeof value === "object" && "error" in value && !allowError) {
    throw new Error(`refused: ${value.error}`);
  }
  return value;
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

console.log(`fun-refactor ${version()}`);
console.log(`sample: ${Object.keys(files).length} files\n`);

// ------------------------------------------------------------------ indexing

let workspace = new Workspace(files);

console.log("workspace");
check("stats", () => {
  const s = json(workspace.stats());
  assert(s.files > 0, "no files indexed");
  assert(s.symbols > 0, "no symbols");
  assert(s.references > 0, "no references");
  assert(s.unparsed.length === 0, `did not parse: ${s.unparsed.join(", ")}`);
  assert(s.unsupported.length === 0, `no grammar for: ${s.unsupported.join(", ")}`);
  return `${s.files} files, ${s.symbols} symbols, ${s.references} references`;
});

// Every grammar the tool has must be represented, or the sample is not a test of it.
const EXPECTED_LANGUAGES = [
  "rust", "go", "zig", "java", "typescript", "tsx", "python", "bash", "html",
  "css", "scss", "sass", "hcl", "yaml", "helm", "xml", "markdown",
];

check("every language is present", () => {
  const s = json(workspace.stats());
  const seen = new Set(s.languages.map(([name]) => name));
  const missing = EXPECTED_LANGUAGES.filter((l) => !seen.has(l));
  assert(missing.length === 0, `no file was recognised as: ${missing.join(", ")}`);
  return [...seen].join(", ");
});

check("files", () => {
  const list = json(workspace.files());
  const unindexed = list.filter((f) => !f.indexed).map((f) => f.path);
  assert(unindexed.length === 0, `not indexed: ${unindexed.join(", ")}`);
  return `${list.length} listed`;
});

check("read round-trips", () => {
  assert(workspace.read("src/ingest.rs") === files["src/ingest.rs"], "text differs");
  return "";
});

check("ast", () => {
  // A tree per language: the structure pane is the only view that has to work for a
  // file the index found nothing in, so it is checked against every grammar.
  const shapes = [];
  for (const [path, expect] of [
    ["src/ingest.rs", "source_file"],
    ["cmd/sink.go", "source_file"],
    ["src/buffer.zig", "source_file"],
    ["web/dashboard.ts", "program"],
    ["web/Panel.tsx", "program"],
    ["scripts/report.py", "module"],
    ["scripts/deploy.sh", "program"],
    ["web/index.html", "document"],
    ["web/dashboard.css", "stylesheet"],
    ["web/theme.scss", "stylesheet"],
    ["infra/main.tf", "config_file"],
    ["ops/pipeline.yaml", "stream"],
    ["chart/templates/deployment.yaml", "stream"],
    ["ops/dashboards.xml", "document"],
    ["docs/README.md", "document"],
  ]) {
    const tree = json(workspace.ast(path));
    assert(tree.kind === expect, `${path}: root is ${tree.kind}, expected ${expect}`);
    assert(tree.children.length > 0, `${path}: the tree has no children`);
    assert(tree.line === 1 && tree.col === 1, `${path}: the root does not start at 1:1`);
    shapes.push(`${expect}(${tree.children.length})`);
  }
  return shapes.length + " trees";
});

check("the parse tree follows an edit", () => {
  // The tree is memoised so the status bar does not reparse on every keystroke. The
  // memo is keyed by the source text, so an edit must miss it — a stale tree would
  // describe the file as it was, silently and convincingly.
  const w = new Workspace({ ...files });
  const before = json(w.ast("src/ingest.rs"));
  const p = at("src/ingest.rs", "validate");
  const applied = json(w.rename(p.path, p.line, p.col, "check_reading_thoroughly"));
  assert(applied.files.length > 0, "the rename changed nothing, so this proves nothing");

  const after = json(w.ast("src/ingest.rs"));
  const text = (node) =>
    node.text !== null && node.text !== undefined
      ? [node.text]
      : node.children.flatMap(text);
  const names = text(after);
  assert(
    names.includes("check_reading_thoroughly"),
    "the tree still shows the old name: the memo served a stale parse",
  );
  assert(
    JSON.stringify(before) !== JSON.stringify(after),
    "the tree is unchanged after an edit that changed the file",
  );
  return "invalidated by the edit";
});

check("at", () => {
  const p = at("src/ingest.rs", "validate");
  const here = json(workspace.at(p.path, p.line, p.col));
  assert(here.coordinate === `${p.path}:${p.line}:${p.col}`, "the coordinate is wrong");
  assert(here.name === "validate", `named ${here.name}`);
  assert(here.kind === "function", `kind was ${here.kind}`);
  assert(here.node, "no tree node reported");
  // A position on nothing must answer, not refuse: the status bar asks on every
  // keystroke, including in whitespace.
  const blank = json(workspace.at("src/ingest.rs", 2, 1));
  assert(blank.name === null, "claimed a symbol on a blank line");
  assert(blank.coordinate, "gave no coordinate for a blank line");
  return `${here.kind} ${here.name}, node ${here.node}`;
});

check("capabilities", () => {
  const caps = json(workspace.capabilities());
  assert(Array.isArray(caps) || typeof caps === "object", "not a list or map");
  return Array.isArray(caps) ? `${caps.length} listed` : "";
});

// --------------------------------------------------------------- navigation

/** Where a name first appears in a file, 1-based, as the editor would report it. */
function at(path, name, occurrence = 1) {
  const text = files[path];
  assert(text !== undefined, `${path} is not in the sample`);
  const lines = text.split("\n");
  let seen = 0;
  for (let i = 0; i < lines.length; i += 1) {
    let from = 0;
    for (;;) {
      const col = lines[i].indexOf(name, from);
      if (col < 0) break;
      const before = lines[i][col - 1] ?? " ";
      const after = lines[i][col + name.length] ?? " ";
      if (!/[A-Za-z0-9_]/.test(before) && !/[A-Za-z0-9_]/.test(after)) {
        seen += 1;
        if (seen === occurrence) return { path, line: i + 1, col: col + 1 };
      }
      from = col + 1;
    }
  }
  throw new Error(`${name} does not appear in ${path}`);
}

// One identifier per language that the tool should be able to say something about.
const SUBJECTS = [
  ["rust", ...Object.values(at("src/ingest.rs", "validate"))],
  // The second occurrence: the first is the doc comment above the function.
  ["go", ...Object.values(at("cmd/collector.go", "Validate", 2))],
  ["zig", ...Object.values(at("src/buffer.zig", "validate"))],
  ["typescript", ...Object.values(at("web/dashboard.ts", "validate"))],
  ["tsx", ...Object.values(at("web/Panel.tsx", "SensorRow"))],
  ["python", ...Object.values(at("scripts/report.py", "validate"))],
  ["bash", ...Object.values(at("scripts/deploy.sh", "require"))],
  ["css", ...Object.values(at("web/dashboard.css", "panel-title"))],
  ["scss", ...Object.values(at("web/theme.scss", "panel-surface"))],
  ["hcl", ...Object.values(at("infra/main.tf", "retention_days"))],
  ["yaml", ...Object.values(at("ops/pipeline.yaml", "test"))],
  ["markdown", ...Object.values(at("docs/README.md", "Layout"))],
];

console.log("\nnavigation");
for (const [language, path, line, col] of SUBJECTS) {
  check(`definition (${language})`, () => {
    const found = json(workspace.definition(path, line, col), { allowError: true });
    if (found.error) return `no symbol at ${path}:${line}:${col} — ${found.error}`;
    return `${found.definitions.length} definition(s)`;
  });
  check(`references (${language})`, () => {
    const refs = json(workspace.references(path, line, col), { allowError: true });
    if (refs.error) return refs.error;
    assert(Array.isArray(refs), "not a list");
    return `${refs.length}`;
  });
  check(`usages (${language})`, () => {
    const found = json(workspace.usages(path, line, col), { allowError: true });
    if (found.error) return found.error;
    assert(Array.isArray(found.usages), "no usages list");
    return `${found.usages.length} here, ${found.same_name_elsewhere.length} elsewhere`;
  });
  check(`symbols (${language})`, () => {
    const outline = json(workspace.symbols(path), { allowError: true });
    if (outline.error) return outline.error;
    assert(Array.isArray(outline), "not a list");
    return `${outline.length}`;
  });
}

check("implementations", () => {
  // `Sink` is implemented by three types, which is the case worth asking about.
  const p = at("cmd/sink.go", "Sink", 3);
  const found = json(workspace.implementations(p.path, p.line, p.col));
  const list = found.definitions ?? found;
  assert(Array.isArray(list), "not a list of definitions");
  assert(list.length > 1, `only ${list.length} — the three Sink implementations were missed`);
  return `${list.length}`;
});

// ------------------------------------------------------------------ analysis

console.log("\nanalysis");
check("graph", () => {
  const g = json(workspace.graph());
  assert(g.functions > 0, "no functions in the call graph");
  return `${g.functions} functions, ${g.edges} edges`;
});

check("callers", () => {
  const p = at("src/ingest.rs", "validate");
  const t = json(workspace.callers(p.path, p.line, p.col, 3));
  assert(typeof t.tree === "string" && t.tree.length > 0, "empty tree");
  return `${t.tree.split("\n").length} lines`;
});

check("callees", () => {
  const p = at("src/main.rs", "report");
  const t = json(workspace.callees(p.path, p.line, p.col, 3));
  assert(typeof t.tree === "string", "no tree");
  return `${t.tree.split("\n").length} lines`;
});

// The playground reads this answer directly. It reported "No symbol at the cursor" for
// every function, because the view expected an `{ ok, value }` envelope that `ok()` does
// not write: a success is the value itself. Nothing tested the shape from JavaScript.
check("graph_around", () => {
  const p = at("src/ingest.rs", "validate");
  const g = json(workspace.graph_around(p.path, p.line, p.col, 2));
  assert(Array.isArray(g.nodes) && g.nodes.length > 0, "no nodes");
  assert(Array.isArray(g.edges), "no edges array");
  assert(typeof g.root === "number", "no root id");
  assert(typeof g.more === "boolean", "no `more` flag");
  assert(
    g.nodes.some((n) => n.id === g.root),
    "the root is not among the nodes",
  );
  for (const n of g.nodes) {
    assert(typeof n.name === "string" && n.name.length > 0, "a node has no name");
    assert(typeof n.file === "string" && typeof n.line === "number", "a node has no place");
    // The column too: clicking a node put the cursor at column 1, on the indentation,
    // and the status bar answered "nothing the index knows at this position".
    assert(typeof n.col === "number" && n.col > 0, `a node has no column: ${n.name}`);
    assert(Math.abs(n.rank) <= 2, `a node sits past the depth asked for: ${n.rank}`);
  }
  const known = new Set(g.nodes.map((n) => n.id));
  for (const e of g.edges) {
    assert(known.has(e.from) && known.has(e.to), "an edge leaves the drawing");
    assert(e.kind === "call" || e.kind === "dispatch", `unknown edge kind ${e.kind}`);
  }
  return `${g.nodes.length} node(s), ${g.edges.length} edge(s)`;
});

// A position that names nothing has to say so in the one shape the view checks.
check("graph_around refuses a position with no symbol", () => {
  const g = json(workspace.graph_around("src/ingest.rs", 1, 1, 2), { allowError: true });
  assert(typeof g.error === "string", "a failure must carry `error`");
  return g.error;
});

check("flow_back", () => {
  const p = at("src/ingest.rs", "celsius", 2);
  const f = json(workspace.flow_back(p.path, p.line, p.col), { allowError: true });
  return f.error ?? "traced";
});

check("flow_forward", () => {
  const p = at("src/ingest.rs", "celsius", 2);
  const f = json(workspace.flow_forward(p.path, p.line, p.col), { allowError: true });
  return f.error ?? "traced";
});

check("impact", () => {
  const p = at("src/ingest.rs", "validate");
  const i = json(workspace.impact(p.path, p.line, p.col), { allowError: true });
  return i.error ?? "computed";
});

check("stitch", () => {
  const s = json(workspace.stitch());
  return Array.isArray(s) ? `${s.length} link(s) across languages` : "computed";
});

check("entrypoints", () => {
  const e = json(workspace.entrypoints());
  assert(Array.isArray(e), "not a list");
  assert(e.length > 0, "no entry points at all, so everything looks dead");
  return `${e.length}`;
});

check("unused", () => {
  const dead = json(workspace.unused());
  assert(Array.isArray(dead), "not a list");
  const names = dead.map((d) => d.name);
  assert(names.includes("hottest"), "missed `hottest`, which nothing calls");
  // Entry points anchor reachability. Without them a `#[test]` reads as dead, which
  // is how the browser build once reported twenty symbols the terminal did not.
  for (const live of ["a_blank_sensor_is_refused", "averages_are_per_sensor"]) {
    assert(!names.includes(live), `\`${live}\` is a #[test], not dead code`);
  }
  return `${dead.length}, including ${names.slice(0, 3).join(", ")}`;
});

check("duplicates", () => {
  const classes = json(workspace.duplicates(30));
  assert(Array.isArray(classes), "not a list");
  return `${classes.length} class(es) at 30 tokens`;
});

// --------------------------------------------------------------- refactoring
//
// Each of these mutates, so each runs against a freshly indexed copy: otherwise a
// later check is testing the result of an earlier one instead of the sample.

function fresh() {
  return new Workspace({ ...files });
}

function applied(name, fn) {
  check(name, () => {
    const w = fresh();
    const result = fn(w);
    const value = json(result, { allowError: true });
    if (value.error) return value.error;
    assert(Array.isArray(value.files), "no file list in the result");
    for (const f of value.files) {
      assert(typeof f.diff === "string", `${f.path} has no diff`);
    }
    return `${value.files.length} file(s) changed`;
  });
}

console.log("\nrefactoring");
applied("rename (rust)", (w) => {
  const p = at("src/ingest.rs", "validate");
  return w.rename(p.path, p.line, p.col, "check_reading");
});
applied("rename (go)", (w) => {
  const p = at("cmd/collector.go", "Validate", 2);
  return w.rename(p.path, p.line, p.col, "CheckReading");
});
applied("rename (python)", (w) => {
  const p = at("scripts/report.py", "validate");
  return w.rename(p.path, p.line, p.col, "check_reading");
});
applied("rename (typescript)", (w) => {
  const p = at("web/dashboard.ts", "validate");
  return w.rename(p.path, p.line, p.col, "checkReading");
});
applied("rename (css class)", (w) => {
  const p = at("web/dashboard.css", "panel-title");
  return w.rename(p.path, p.line, p.col, "panel-heading");
});
applied("rename (hcl variable)", (w) => {
  const p = at("infra/main.tf", "retention_days");
  return w.rename(p.path, p.line, p.col, "keep_days");
});
applied("rename (helm value)", (w) => {
  const p = at("chart/values.yaml", "replicaCount");
  return w.rename(p.path, p.line, p.col, "replicas");
});

applied("extract_variable", (w) => {
  // `celsius * 9.0 / 5.0 + 32.0` in the Rust `fahrenheit`.
  const lines = files["src/convert.rs"].split("\n");
  const line = lines.findIndex((l) => l.includes("celsius * 9.0")) + 1;
  const from = lines[line - 1].indexOf("celsius * 9.0") + 1;
  const to = lines[line - 1].indexOf("+ 32.0") + "+ 32.0".length + 1;
  return w.extract_variable("src/convert.rs", `${line}:${from}-${line}:${to}`, "scaled");
});

applied("extract_function", (w) => {
  const lines = files["scripts/report.py"].split("\n");
  const start = lines.findIndex((l) => l.includes("sums = defaultdict")) + 1;
  const end = lines.findIndex((l) => l.includes("counts[reading[\"sensor\"]] += 1")) + 1;
  return w.extract_function("scripts/report.py", `${start}:1-${end}:100`, "accumulate");
});

applied("inline_variable", (w) => {
  // The `let limits = …` in `main`, not the parameter of the same name in `report`.
  const p = at("src/main.rs", "limits", 3);
  return w.inline_variable(p.path, p.line, p.col);
});

applied("inline_call", (w) => {
  const p = at("src/main.rs", "sample", 2);
  return w.inline_call(p.path, p.line, p.col);
});

applied("signature (add a parameter)", (w) => {
  const p = at("src/convert.rs", "fahrenheit");
  return w.signature(p.path, p.line, p.col, "add:1:offset: f64:0.0");
});

applied("move_symbol", (w) => {
  const p = at("src/ingest.rs", "hottest");
  return w.move_symbol(p.path, p.line, p.col, "src/convert.rs");
});

applied("organize_imports", (w) => w.organize_imports("scripts/report.py"));

applied("delete", (w) => {
  const p = at("src/ingest.rs", "hottest");
  return w.delete(p.path, p.line, p.col);
});

applied("remove_flag", (w) => w.remove_flag("REPORT_IN_CELSIUS", true));

/** The `if !REPORT_IN_CELSIUS` in `unit()`, which invert-if should offer to flip. */
function invertibleIf() {
  const lines = files["src/convert.rs"].split("\n");
  const index = lines.findIndex((l) => l.includes("if !REPORT_IN_CELSIUS"));
  assert(index >= 0, "the sample no longer has a negated condition to invert");
  return { line: index + 1, col: lines[index].indexOf("if") + 1 };
}

check("rewrites_at", () => {
  const where = invertibleIf();
  const list = json(workspace.rewrites_at("src/convert.rs", where.line, where.col));
  assert(Array.isArray(list), "not a list");
  assert(list.length > 0, "nothing offered on a negated if, where invert-if applies");
  return list.map((r) => r.id ?? r.name ?? r).join(", ");
});

applied("rewrite (invert-if)", (w) => {
  const where = invertibleIf();
  return w.rewrite("src/convert.rs", where.line, where.col, "invert-if");
});

applied("restructure", (w) =>
  w.restructure("python", "$a is not None", "$a != None"));

console.log(`\n${checks - failures}/${checks} passed`);
process.exit(failures === 0 ? 0 : 1);
