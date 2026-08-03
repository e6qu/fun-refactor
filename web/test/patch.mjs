/**
 * The patch the playground downloads is one `git apply` will take.
 *
 * A patch that does not apply fails silently in the worst way: the button produces a
 * file, the file looks like a diff, and it is rejected somewhere else entirely, long
 * after the session that made it is gone. So this does not inspect the text — it runs
 * the real refactorings against the bundled sample, writes the patch out, and asks
 * git.
 *
 *     node web/test/patch.mjs
 */

import { execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");

const { default: init, Workspace } = await import(join(root, "src/wasm/fun_refactor.js"));
await init({
  module_or_path: readFileSync(join(root, "src/wasm/fun_refactor_bg.wasm")),
});

// The page compiles `patch.ts`; Node cannot import TypeScript, so the types are
// stripped. The body is plain JavaScript, which is what makes that safe.
const source = readFileSync(join(root, "src/patch.ts"), "utf8");
const asJs = source
  .replace(/^export /gm, "")
  .replace(/: (string|number|Record<string, string>|string\[\])(?=[,)])/g, "")
  .replace(/\): string \{/g, ") {")
  .replace(/const CONTEXT: number/, "const CONTEXT");
const { diffOf, patchOf } = await import(
  "data:text/javascript," + encodeURIComponent(asJs + "\nexport { diffOf, patchOf };")
);

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
const original = {};
for (const path of walk(sampleRoot)) {
  original[relative(sampleRoot, path)] = readFileSync(path, "utf8");
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

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

/** Lay the sample down as a git repository, apply the patch, and see. */
function gitAccepts(patch, changed) {
  const dir = mkdtempSync(join(tmpdir(), "fr-patch-"));
  try {
    for (const [path, text] of Object.entries(original)) {
      mkdirSync(join(dir, dirname(path)), { recursive: true });
      writeFileSync(join(dir, path), text);
    }
    const git = (...args) => execFileSync("git", args, { cwd: dir, encoding: "utf8" });
    git("init", "-q");
    git("config", "user.email", "test@example.com");
    git("config", "user.name", "test");
    git("add", "-A");
    git("commit", "-qm", "sample");

    writeFileSync(join(dir, "session.patch"), patch);
    // --check first: it reports why, where `apply` alone would only fail.
    git("apply", "--check", "session.patch");
    git("apply", "session.patch");

    // Applying it must reproduce the workspace exactly, not merely succeed.
    for (const [path, expected] of Object.entries(changed)) {
      const got = readFileSync(join(dir, path), "utf8");
      assert(
        got === expected,
        `${path} after applying the patch is not what the workspace holds`,
      );
    }
    return git("diff", "--stat", "HEAD").trim().split("\n").pop().trim();
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

/** Run a refactoring and return the workspace after it. */
function after(act) {
  const workspace = new Workspace({ ...original });
  const result = JSON.parse(act(workspace));
  assert(!result.error, `the refactoring refused: ${result.error}`);
  assert(result.files.length > 0, "the refactoring changed nothing, so there is no patch");
  const files = { ...original };
  for (const f of result.files) files[f.path] = workspace.read(f.path);
  return files;
}

/** Where a name first appears in a file, 1-based. */
function at(path, name, occurrence = 1) {
  const lines = original[path].split("\n");
  let seen = 0;
  for (let i = 0; i < lines.length; i += 1) {
    let from = 0;
    for (;;) {
      const col = lines[i].indexOf(name, from);
      if (col < 0) break;
      const before = lines[i][col - 1] ?? " ";
      const behind = lines[i][col + name.length] ?? " ";
      if (!/[A-Za-z0-9_]/.test(before) && !/[A-Za-z0-9_]/.test(behind)) {
        seen += 1;
        if (seen === occurrence) return { path, line: i + 1, col: col + 1 };
      }
      from = col + 1;
    }
  }
  throw new Error(`${name} does not appear in ${path}`);
}

console.log("patch");

check("an identical file produces no diff at all", () => {
  assert(diffOf("a.txt", "same\n", "same\n") === "", "produced a diff for no change");
  return "";
});

const CASES = [
  [
    "rename across one file",
    (w) => {
      const p = at("src/ingest.rs", "validate");
      return w.rename(p.path, p.line, p.col, "check_reading");
    },
  ],
  [
    "rename a CSS class, which spans three files",
    (w) => {
      const p = at("web/dashboard.css", "panel-title");
      return w.rename(p.path, p.line, p.col, "panel-heading");
    },
  ],
  [
    "rename a Helm value, which spans the chart",
    (w) => {
      const p = at("chart/values.yaml", "replicaCount");
      return w.rename(p.path, p.line, p.col, "replicas");
    },
  ],
  [
    "an edit on the very first line",
    (w) => {
      const p = at("scripts/report.py", "MIN_CELSIUS");
      return w.rename(p.path, p.line, p.col, "FLOOR_CELSIUS");
    },
  ],
  [
    "a deletion, which removes lines rather than changing them",
    (w) => {
      const p = at("src/ingest.rs", "hottest");
      return w.delete(p.path, p.line, p.col);
    },
  ],
  [
    "retiring a flag, which rewrites branches in two files",
    (w) => w.remove_flag("REPORT_IN_CELSIUS", true),
  ],
];

for (const [name, act] of CASES) {
  check(name, () => {
    const files = after(act);
    const changed = Object.fromEntries(
      Object.keys(files).filter((p) => files[p] !== original[p]).map((p) => [p, files[p]]),
    );
    const patch = patchOf(Object.keys(changed), original, files);
    assert(patch.length > 0, "no patch text for a change that happened");
    return `${Object.keys(changed).length} file(s), git says: ${gitAccepts(patch, changed)}`;
  });
}

check("several refactorings in one session make one patch", () => {
  // The real shape: a person renames, then deletes, then downloads once.
  const workspace = new Workspace({ ...original });
  const files = { ...original };
  const steps = [
    () => {
      const p = at("src/ingest.rs", "validate");
      return workspace.rename(p.path, p.line, p.col, "check_reading");
    },
    () => {
      const p = at("src/ingest.rs", "hottest");
      return workspace.delete(p.path, p.line, p.col);
    },
    () => workspace.organize_imports("scripts/report.py"),
  ];
  for (const step of steps) {
    const result = JSON.parse(step());
    assert(!result.error, `a step refused: ${result.error}`);
    for (const f of result.files) files[f.path] = workspace.read(f.path);
  }
  const changed = Object.fromEntries(
    Object.keys(files).filter((p) => files[p] !== original[p]).map((p) => [p, files[p]]),
  );
  assert(Object.keys(changed).length >= 2, "expected more than one file touched");
  const patch = patchOf(Object.keys(changed), original, files);
  return `${Object.keys(changed).length} file(s), git says: ${gitAccepts(patch, changed)}`;
});

console.log(`\n${checks - failures}/${checks} passed`);
process.exit(failures === 0 ? 0 : 1);
