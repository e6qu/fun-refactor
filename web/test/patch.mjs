/** Exercise the browser's Rust patch exporter against Git itself. */

import { execFileSync } from "node:child_process";
import {
  mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const { default: init, Workspace } = await import(join(root, "src/wasm/fun_refactor.js"));
await init({ module_or_path: readFileSync(join(root, "src/wasm/fun_refactor_bg.wasm")) });

function walk(dir) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}
const sampleRoot = join(root, "sample");
const original = Object.fromEntries(
  walk(sampleRoot).map((path) => [relative(sampleRoot, path), readFileSync(path, "utf8")]),
);

let failures = 0;
let checks = 0;
function assert(condition, message) {
  if (!condition) throw new Error(message);
}
function check(name, fn) {
  checks += 1;
  try {
    const note = fn();
    console.log(`  ok   ${name}${note ? ` — ${note}` : ""}`);
  } catch (error) {
    failures += 1;
    console.log(`  FAIL ${name}: ${error.message}`);
  }
}
function at(path, name) {
  const lines = original[path].split("\n");
  for (let line = 0; line < lines.length; line += 1) {
    const col = lines[line].indexOf(name);
    if (col >= 0) return { path, line: line + 1, col: col + 1 };
  }
  throw new Error(`${name} does not appear in ${path}`);
}

/** Apply a browser export to a real Git worktree and compare every loaded file. */
function gitAccepts(patch, workspace) {
  const dir = mkdtempSync(join(tmpdir(), "fr-browser-patch-"));
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
    git("commit", "-qm", "basis");
    writeFileSync(join(dir, "session.patch"), patch);
    git("apply", "--check", "session.patch");
    git("apply", "session.patch");
    for (const file of JSON.parse(workspace.files())) {
      assert(
        readFileSync(join(dir, file.path), "utf8") === workspace.read(file.path),
        `${file.path} differs after applying the exported patch`,
      );
    }
    return git("diff", "--stat", "HEAD").trim().split("\n").pop().trim();
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const CASES = [
  ["rename across one file", (w) => {
    const p = at("src/ingest.rs", "validate");
    return w.rename(p.path, p.line, p.col, "check_reading");
  }],
  ["rename a CSS class across files", (w) => {
    const p = at("web/dashboard.css", "panel-title");
    return w.rename(p.path, p.line, p.col, "panel-heading");
  }],
  ["rename a Helm value across the chart", (w) => {
    const p = at("chart/values.yaml", "replicaCount");
    return w.rename(p.path, p.line, p.col, "replicas");
  }],
  ["delete a definition", (w) => {
    const p = at("src/ingest.rs", "hottest");
    return w.delete(p.path, p.line, p.col);
  }],
  ["retire a flag across files", (w) => w.remove_flag("REPORT_IN_CELSIUS", true)],
];

console.log("browser patch export");
for (const [name, act] of CASES) {
  check(name, () => {
    const workspace = new Workspace({ ...original });
    const applied = JSON.parse(act(workspace));
    assert(!applied.error, `the refactoring refused: ${applied.error}`);
    const exported = JSON.parse(workspace.patch());
    assert(exported.schema === "fr-memory-patch-1", "wrong export schema");
    assert(exported.patch.length > 0, "an applied transaction exported no patch");
    return gitAccepts(exported.patch, workspace);
  });
}

check("undo and redo change the cumulative export", () => {
  const workspace = new Workspace({ ...original });
  const first = at("src/ingest.rs", "validate");
  const second = at("web/dashboard.css", "panel-title");
  assert(!JSON.parse(workspace.rename(first.path, first.line, first.col, "check_reading")).error);
  assert(!JSON.parse(workspace.rename(second.path, second.line, second.col, "panel-heading")).error);
  const both = JSON.parse(workspace.patch()).patch;
  assert(!JSON.parse(workspace.undo(2)).error, "undo refused");
  const one = JSON.parse(workspace.patch()).patch;
  assert(one.length < both.length, "undo did not remove the latest transaction from the patch");
  assert(!JSON.parse(workspace.redo(2)).error, "redo refused");
  assert(JSON.parse(workspace.patch()).patch === both, "redo did not restore the export");
  return gitAccepts(both, workspace);
});

console.log(`\n${checks - failures}/${checks} passed`);
process.exit(failures === 0 ? 0 : 1);
