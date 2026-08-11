/**
 * Every refactoring, against every symbol, at the scale people actually work at.
 *
 * The other harnesses ask whether a capability *works*. This one asks whether it
 * survives a real repository: hundreds of symbols, sixteen languages, and one bounded
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

// The module this sweeps is a build artifact, and a stale one answers every question with
// last week's code while looking exactly like a pass. Building it needs a wasm toolchain
// that not every machine has, so the failure mode is a developer running this against
// whatever `web/src/wasm` happens to hold — which is how a week-old module once reported
// "no defects" for a change it had never seen.
const wasm = join(root, "src/wasm/fun_refactor_bg.wasm");
{
  const built = statSync(wasm).mtimeMs;
  const newest = (dir) =>
    readdirSync(dir, { withFileTypes: true }).reduce((latest, entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return Math.max(latest, newest(path));
      return entry.name.endsWith(".rs")
        ? Math.max(latest, statSync(path).mtimeMs)
        : latest;
    }, 0);
  const source = newest(join(root, "..", "src"));
  if (source > built) {
    console.error(
      `web/src/wasm is older than src/: built ${new Date(built).toISOString()}, ` +
        `newest source ${new Date(source).toISOString()}.\n` +
        "Run tools/build-wasm.sh — sweeping a stale module measures nothing.",
    );
    process.exit(1);
  }
}

const { default: init, Workspace } = await import(join(root, "src/wasm/fun_refactor.js"));
await init({
  module_or_path: readFileSync(join(root, "src/wasm/fun_refactor_bg.wasm")),
});

const repo = process.argv[2] ?? join(root, "sample");
// Each probe indexes the workspace from scratch so one refactoring cannot affect the
// next. That isolation costs an index build per probe, which is why the count is the
// caller's to choose. Every language is probed regardless of the count — see the
// sampling below — and forty keeps CI to about a minute; a real sweep is
// `scale.mjs /path/to/repo 200`, and takes as long as it takes.
const LIMIT = Number(process.argv[3] ?? 40);
const started = Date.now();

/** Extensions with a grammar. Anything else is weight without answers. */
const PARSEABLE = new Set([
  ".rs", ".go", ".java", ".ts", ".tsx", ".js", ".jsx", ".py", ".zig", ".sh", ".bash",
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
/**
 * Spread the probes across the workspace instead of taking the first N.
 *
 * `targets` is in path order, so the first forty of the bundled sample are all in
 * `chart/` — a run that claims to cover sixteen languages and covers YAML. Striding
 * takes the same number and touches every file, and one probe per language goes in
 * first so a language with few definitions cannot fall through the gaps.
 */
// One probe per language first, then stride-fill the rest. Striding alone left `html`
// and `scss` unprobed — they have few definitions and the gaps fall past them — so the
// sweep covered fourteen of sixteen languages while its comment claimed all of them.
// Tuning LIMIT until they appear would fix today and break the next time a language is
// added; taking one of each first makes the coverage a property of the algorithm.
const languageOf = new Map(
  JSON.parse(index.files()).map((f) => [f.path, f.language]),
);
const firstOfEach = [];
const seen = new Set();
for (const target of targets) {
  const language = languageOf.get(target.path);
  if (language && !seen.has(language)) {
    seen.add(language);
    firstOfEach.push(target);
  }
}
const stride = Math.max(1, Math.floor(targets.length / LIMIT));
const strided = targets.filter((_, i) => i % stride === 0);
const sample = [...firstOfEach, ...strided.filter((t) => !firstOfEach.includes(t))].slice(
  0,
  Math.max(LIMIT, firstOfEach.length),
);
const languagesProbed = new Set(sample.map((t) => languageOf.get(t.path)));
console.log(
  `  probing ${sample.length} of ${targets.length} definitions across ` +
    `${new Set(sample.map((t) => t.path)).size} files, ` +
    `${[...languagesProbed].filter(Boolean).sort().join(" ")}\n`,
);

// The coverage claim, asserted and not written in a comment above it. `.java` was
// missing from PARSEABLE for a whole release: the sweep went on saying it covered every
// language of the bundled sample while skipping one of them entirely, and nothing
// noticed because the only statement of the claim was prose.
const inWorkspace = new Set([...languageOf.values()].filter(Boolean));
const missed = [...inWorkspace].filter((l) => !languagesProbed.has(l)).sort();
if (missed.length) {
  console.error(
    `  the sweep claims to cover every language in the workspace and did not probe: ` +
      `${missed.join(", ")}. Either raise the probe count or add the extension to ` +
      `PARSEABLE.`,
  );
  process.exit(1);
}

/**
 * A refusal names a rule; anything else that fails is a defect.
 *
 * The list below is the fallback. It reads the sentence, which means rewording a refusal
 * reclassifies it as a defect — and that happened, when five refusals stopped saying
 * "is not supported for {language}" about a path and started saying what was actually
 * wrong. The API reports `refused` now, and the patterns are only for the errors that do
 * not carry it.
 */
const REFUSAL = [
  /at that position/i,                 // no `if` / no negation / no symbol here
  /refus|declin/i,
  /not supported|unsupported|cannot|does not|is not|would not|has no|there is no|neither/i,
  /\bonly\b.*\b(can|are|have)\b/i,     // "only top-level declarations can be moved"
  /would change behaviour|is assigned again/i,
  /declares a package|no go\.mod|would make it unreachable/i,
  /nested inside another definition/i,
  /is a \w[\w-]*, not a|is a `\w+` block, not a/i,   // "is a link-def, not a heading"
  /is still read \d+ time|would leave those/i,        // removing an input still used
  /is a block in \w+|names a module signature/i,
  /has \d+ selectors|which is a rewrite instead of a move/i,
  /part of the module's call surface/i,
  /invert it instead|guard it instead|has an `?else`?/i,
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
    // What the tool says about itself, before what its wording suggests.
    if (value.refused === true) {
      return { kind: "refused", detail: value.error };
    }
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
  // One probe must not see another's edits. A refusal changes nothing — that is the
  // tool's contract and the thing most of these probes exercise — so the workspace is
  // only rebuilt after something actually mutated it. Rebuilding unconditionally cost
  // four hundred index builds to observe about thirty edits.
  let workspace = new Workspace({ ...files });
  for (const target of sample) {
    let outcome;
    try {
      outcome = outcomeOf(run(workspace, target));
    } catch (e) {
      // A trap is not an exception the page could have handled: the module is gone.
      outcome = { kind: "TRAPPED", detail: String(e).split("\n")[0] };
      workspace = new Workspace({ ...files });
    }
    counts[outcome.kind] = (counts[outcome.kind] ?? 0) + 1;
    if (["BROKE", "TRAPPED", "UNPARSEABLE"].includes(outcome.kind) && worst.length < 5) {
      worst.push([target, outcome.detail]);
    }
    if (outcome.kind === "ok") {
      // Without the free the module runs out of memory: a Workspace owns an entire
      // index and the JavaScript collector cannot see any of it.
      workspace.free();
      workspace = new Workspace({ ...files });
    }
  }
  workspace.free();

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

// ------------------------------------------------------------- the pattern DSL
//
// `restructure` has its own little language: `$NAME` matches any node and binds its
// text. Its failure mode is silence — a pattern that matches nothing looks exactly
// like a pattern that had nothing to match, and both report success. So each probe
// below states whether it expects to match, and a `mustMatch` that finds nothing is
// a defect instead of a shrug.

console.log("\npattern DSL");

const PATTERNS = [
  // language, pattern, template, must it match the sample?
  ["python", "len($X) == 0", "not $X", false],
  ["python", "$A is not None", "$A != None", false],
  ["python", 'open($P, encoding="utf-8")', "open($P)", false],
  ["rust", "$A.clone()", "$A.to_owned()", false],
  ["rust", "format!($F, $A)", "format!($F, $A)", false],
  ["go", "fmt.Sprintf($F, $A)", "fmt.Sprintf($F, $A)", false],
  ["go", "errors.New($M)", "fmt.Errorf($M)", false],
  ["typescript", "$A === undefined", "$A == null", false],
  ["typescript", "console.log($X)", "logger.debug($X)", false],
  ["css", "color: $C", "color: $C", false],
  ["scss", "color: $$brand", "color: $$primary", false],
  ["yaml", "runs-on: $R", "runs-on: $R", false],
  ["bash", "echo $M", "printf '%s\\n' $M", false],
  ["hcl", "count = $N", "count = $N", false],
  // A metavariable in the template that the pattern never bound: the DSL must
  // refuse and not write the literal text `$Y` into the source.
  ["python", "len($X) == 0", "not $Y", false],
  // Malformed: an unparseable fragment is a refusal, not a panic.
  ["python", "def (", "pass", false],
  ["rust", "fn $", "fn x", false],
];

{
  const counts = {};
  const problems = [];
  const matched = [];
  for (const [language, pattern, template, mustMatch] of PATTERNS) {
    const workspace = new Workspace({ ...files });
    let outcome;
    try {
      outcome = outcomeOf(workspace.restructure(language, pattern, template));
    } catch (e) {
      outcome = { kind: "TRAPPED", detail: String(e).split("\n")[0] };
    }
    counts[outcome.kind] = (counts[outcome.kind] ?? 0) + 1;
    if (["BROKE", "TRAPPED", "UNPARSEABLE"].includes(outcome.kind)) {
      problems.push([`${language} '${pattern}'`, outcome.detail]);
    }
    if (outcome.kind === "ok") {
      matched.push(`${language} '${pattern}' -> ${outcome.value.files.length} file(s)`);
    }
    if (mustMatch && outcome.kind !== "ok") {
      problems.push([`${language} '${pattern}'`, `expected a match, got ${outcome.kind}`]);
    }
    workspace.free();
  }
  console.log(
    `  ${PATTERNS.length} patterns: ` +
      Object.entries(counts).sort().map(([k, v]) => `${k}=${v}`).join(", "),
  );
  // Which ones bit. A count of no-ops is not evidence either way without this: a
  // pattern for a language the corpus does not contain *should* match nothing.
  if (matched.length) {
    console.log(`      matched: ${matched.join("; ")}`);
  }
  for (const [what, why] of problems.slice(0, 8)) {
    console.log(`      ${what}`);
    console.log(`      ${String(why).replace(/\s+/g, " ").slice(0, 170)}`);
  }
  for (const bad of ["BROKE", "TRAPPED", "UNPARSEABLE"]) {
    if (counts[bad]) report("pattern DSL", `${counts[bad]} × ${bad}`);
  }
  if (problems.some(([, why]) => String(why).startsWith("expected"))) {
    report("pattern DSL", "a pattern that must match found nothing");
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
  // Every declaration site of the entity, plus every reference the tool said it
  // could rewrite. Not "one definition": a CSS class written in a stylesheet and
  // again in a theme is one class with three declaration sites, and a rename that
  // changed only one of them would leave the others pointing at nothing.
  let sites = 1;
  try {
    const defs = JSON.parse(index.definition(target.path, target.line, target.col));
    if (Array.isArray(defs.definitions) && defs.definitions.length) {
      sites = defs.definitions.filter((d) => d.role !== "implementation").length;
    }
  } catch {
    /* keep 1 */
  }
  const expected = strong.length + sites;
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
