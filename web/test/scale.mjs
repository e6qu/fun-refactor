/**
 * Every refactoring, against every symbol, at the scale people actually work at.
 *
 * The other harnesses ask whether a capability *works*. This one asks whether it
 * survives a real repository: hundreds of symbols, fifteen languages, and one bounded
 * wasm heap. It checks three things the CLI's own tests cannot:
 *
 *   1. Nothing traps. A Rust panic or an allocation failure in wasm is
 *      `RuntimeError: unreachable` — not an exception a page can recover from, but
 *      the end of that workspace. This is how the leaked-`Workspace` bug was found:
 *      a `Workspace` owns an entire index in Rust memory, dropping the JavaScript
 *      handle frees none of it, and the module aborted a few hundred probes in.
 *
 *   2. A rename rewrites everything it claimed it could. The tool reports the
 *      confidence of every reference and rewrites the top two tiers; if it resolved
 *      twelve references as `exact` and wrote eleven, it has silently broken the
 *      code, and no reparse check would notice because the result still parses.
 *
 *   3. Nothing succeeds while doing nothing. A refactoring that returns no error and
 *      no change is the failure a person actually sees: they pressed the button and
 *      the code is the same.
 *
 * By default it runs against the bundled sample, which is what CI can afford. Point
 * it at a clone to do the real thing:
 *
 *     node web/test/scale.mjs
 *     node web/test/scale.mjs /path/to/requests 400
 */

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, extname } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const { default: init, Workspace } = await import(join(root, "src/wasm/fun_refactor.js"));
await init({
  module_or_path: readFileSync(join(root, "src/wasm/fun_refactor_bg.wasm")),
});

const repo = process.argv[2] ?? join(root, "sample");
// Each probe indexes the workspace from scratch so one refactoring cannot affect the
// next. That isolation costs an index build per probe, so the count is the caller's
// to choose: 120 against the bundled sample is seconds, 120 against a 400-file
// repository is a quarter of an hour.
const LIMIT = Number(process.argv[3] ?? 120);
const started = Date.now();

/** Extensions with a grammar. Anything else is weight without answers. */
const PARSEABLE = new Set([
  ".rs", ".go", ".ts", ".tsx", ".js", ".jsx", ".py", ".zig", ".sh", ".bash",
  ".html", ".htm", ".css", ".scss", ".tf", ".tfvars", ".yaml", ".yml", ".xml", ".md",
]);

function walk(dir, out = []) {
  for (const entry of readdirSync(dir)) {
    if (entry === ".git" || entry === "node_modules" || entry === "target") continue;
    const path = join(dir, entry);
    try {
      // A real repository can contain a broken symlink — helm's testdata does — and
      // that is not a reason to stop looking at the other twelve thousand files.
      if (statSync(path).isDirectory()) walk(path, out);
      else out.push(path);
    } catch {
      /* unreadable entry */
    }
  }
  return out;
}

const files = {};
for (const path of walk(repo)) {
  if (!PARSEABLE.has(extname(path))) continue;
  try {
    if (statSync(path).size > 400_000) continue;
    files[relative(repo, path)] = readFileSync(path, "utf8");
  } catch {
    /* unreadable file */
  }
}

const NEW_NAME = "fr_scale_probe";
let failures = 0;

function report(name, detail) {
  failures += 1;
  console.log(`  FAIL ${name}: ${detail}`);
}

console.log(`${repo.split("/").pop()}: ${Object.keys(files).length} files`);

const index = new Workspace(files);
const stats = JSON.parse(index.stats());
console.log(
  `  ${stats.files} indexed, ${stats.symbols} symbols, ${stats.references} references` +
    ` — ${stats.languages.map(([l, n]) => `${l}:${n}`).join(" ")}`,
);

// Every definition, at the positions the outline offers — the same ones the page uses.
const targets = [];
for (const path of Object.keys(files)) {
  let symbols;
  try {
    symbols = JSON.parse(index.symbols(path));
  } catch {
    continue;
  }
  if (Array.isArray(symbols)) for (const s of symbols) targets.push({ path, ...s });
}
const sample = targets.slice(0, LIMIT);
console.log(`  probing ${sample.length} of ${targets.length} definitions\n`);

/** A refusal names a rule; anything else that fails is a defect. */
const REFUSAL = [
  /at that position/i,                 // no `if` / no negation / no symbol here
  /refus|declin/i,
  /not supported|unsupported|cannot|does not|is not|would not|has no|there is no|neither/i,
  /\bonly\b.*\b(can|are|have)\b/i,     // "only top-level declarations can be moved"
  /would change behaviour|is assigned again/i,
  /declares a package|no go\.mod|would make it unreachable/i,
  /nested inside another definition/i,
  /is a \w[\w-]*, not a/i,              // "is a link-def, not a heading"
  /nothing to act on|nothing is|no reference|not part of|already defined|is already/i,
  /is a \w+;|outside|no crate/i,
];
const isRefusal = (text) => REFUSAL.some((r) => r.test(text));

function outcomeOf(raw) {
  let value;
  try {
    value = JSON.parse(raw);
  } catch (e) {
    return { kind: "UNPARSEABLE", detail: String(e) };
  }
  if (value && typeof value === "object" && "error" in value) {
    return isRefusal(value.error)
      ? { kind: "refused", detail: value.error }
      : { kind: "BROKE", detail: value.error };
  }
  if (Array.isArray(value.files) && value.files.length === 0) {
    return { kind: "NO-OP", detail: "no error and no change" };
  }
  return { kind: "ok", value };
}

const OPERATIONS = {
  rename: (w, t) => w.rename(t.path, t.line, t.col, NEW_NAME),
  delete: (w, t) => w.delete(t.path, t.line, t.col),
  "inline variable": (w, t) => w.inline_variable(t.path, t.line, t.col),
  "inline call": (w, t) => w.inline_call(t.path, t.line, t.col),
  "organize imports": (w, t) => w.organize_imports(t.path),
  move: (w, t) => w.move_symbol(t.path, t.line, t.col, `zz_probe${extname(t.path)}`),
  signature: (w, t) => w.signature(t.path, t.line, t.col, "remove:0"),
  "invert-if": (w, t) => w.rewrite(t.path, t.line, t.col, "invert-if"),
  "guard-clause": (w, t) => w.rewrite(t.path, t.line, t.col, "guard-clause"),
  "de-morgan": (w, t) => w.rewrite(t.path, t.line, t.col, "de-morgan"),
};

for (const [operation, run] of Object.entries(OPERATIONS)) {
  const counts = {};
  const worst = [];
  for (const target of sample) {
    const workspace = new Workspace({ ...files });
    let outcome;
    try {
      outcome = outcomeOf(run(workspace, target));
    } catch (e) {
      // A trap is not an exception the page could have handled: the module is gone.
      outcome = { kind: "TRAPPED", detail: String(e).split("\n")[0] };
    }
    counts[outcome.kind] = (counts[outcome.kind] ?? 0) + 1;
    if (["BROKE", "TRAPPED", "UNPARSEABLE"].includes(outcome.kind) && worst.length < 5) {
      worst.push([target, outcome.detail]);
    }
    // Without this the module runs out of memory: a Workspace owns an entire index
    // and the JavaScript collector cannot see any of it.
    workspace.free();
  }

  const summary = Object.entries(counts)
    .sort()
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
  console.log(`  ${operation.padEnd(18)} ${summary}`);
  for (const [t, detail] of worst) {
    console.log(`      ${t.path}:${t.line}:${t.col} [${t.kind} ${t.name}]`);
    console.log(`      ${String(detail).replace(/\s+/g, " ").slice(0, 170)}`);
  }
  for (const bad of ["BROKE", "TRAPPED", "UNPARSEABLE"]) {
    if (counts[bad]) report(operation, `${counts[bad]} × ${bad}`);
  }
}

// --------------------------------------------------- did the rename finish the job?

console.log("\nrename fidelity");
let checked = 0;
let mismatched = 0;
const examples = [];
for (const target of sample) {
  let refs;
  try {
    refs = JSON.parse(index.references(target.path, target.line, target.col));
  } catch {
    continue;
  }
  if (!Array.isArray(refs)) continue;
  const strong = refs.filter(
    (r) => r.confidence === "exact" || r.confidence === "import-qualified",
  );

  const workspace = new Workspace({ ...files });
  const outcome = outcomeOf(workspace.rename(target.path, target.line, target.col, NEW_NAME));
  if (outcome.kind !== "ok") {
    workspace.free();
    continue;
  }
  checked += 1;

  let written = 0;
  for (const changed of outcome.value.files) {
    const after = workspace.read(changed.path);
    written += (after.match(new RegExp(`\\b${NEW_NAME}\\b`, "g")) ?? []).length;
  }
  // The definition site, plus every reference the tool said it could rewrite.
  const expected = strong.length + 1;
  if (written !== expected) {
    mismatched += 1;
    if (examples.length < 6) {
      examples.push(
        `${target.path}:${target.line}:${target.col} [${target.kind} ${target.name}] ` +
          `wrote ${written}, expected ${expected}`,
      );
    }
  }
  workspace.free();
}
console.log(`  ${checked} renames checked, ${mismatched} wrote a different number of sites`);
for (const e of examples) console.log(`      ${e}`);
if (mismatched) {
  report("rename fidelity", `${mismatched} renames did not rewrite what they resolved`);
}

const seconds = ((Date.now() - started) / 1000).toFixed(1);
console.log(
  failures === 0 ? `\nno defects (${seconds}s)` : `\n${failures} defect(s) (${seconds}s)`,
);
process.exit(failures === 0 ? 0 : 1);
