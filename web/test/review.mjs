/** Review, restart and deliver a browser edit using the shipped WASM bindings. */
import assert from "node:assert/strict";
import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { ACTIONS } from "../src/actions.ts";
import init, { Workspace } from "../src/wasm/fun_refactor.js";

await init({ module_or_path: readFileSync(new URL("../src/wasm/fun_refactor_bg.wasm", import.meta.url)) });
const source = "def leaf():\n    return 7\ndef main():\n    return leaf()\n";
const workspace = new Workspace({ "a.py": source });
const checkpoint = workspace.session();
const context = { path: "a.py", line: 1, col: 5, range: "2:5-2:13",
  subject: "leaf", answer: "renamed", language: "python" };
const rename = ACTIONS.find(action => action.id === "rename");
const review = rename.run(workspace, context);
assert.equal(JSON.parse(review).review.schema, "fr-browser-review-1");
assert.equal(workspace.session(), checkpoint);
const restarted = Workspace.from_session(checkpoint);
const applied = JSON.parse(restarted.apply_review(review));
assert.equal(applied.error, undefined, JSON.stringify(applied));
assert.equal(restarted.read("a.py"), source.replaceAll("leaf", "renamed"));
assert.equal(JSON.parse(restarted.graph()).details.coverage.external_callers_known, false);
const accepted = restarted.session();
assert.ok(JSON.parse(restarted.apply_review(review)).error);
assert.equal(restarted.session(), accepted);
assert.equal(JSON.parse(restarted.undo(applied.transaction)).error, undefined);
assert.equal(restarted.read("a.py"), source);
assert.equal(JSON.parse(restarted.redo(applied.transaction)).error, undefined);

// Every editing action must preview or refuse without touching the live workspace.
for (const action of ACTIONS.filter(action => action.mutates)) {
  const before = workspace.session();
  const result = JSON.parse(action.run(workspace, context));
  assert.ok(result.error || result.review?.schema === "fr-browser-review-1", action.id);
  assert.equal(workspace.session(), before, action.id);
}

const dir = mkdtempSync(join(tmpdir(), "fr-reviewed-patch-"));
try {
  const git = (...args) => execFileSync("git", args, { cwd: dir, encoding: "utf8" });
  writeFileSync(join(dir, "a.py"), source);
  git("init", "-q");
  const patch = JSON.parse(review).review.patch.patch;
  writeFileSync(join(dir, "change.patch"), patch);
  git("apply", "--check", "change.patch");
  git("apply", "change.patch");
  assert.equal(readFileSync(join(dir, "a.py"), "utf8"), restarted.read("a.py"));
  assert.equal(execFileSync("python3", ["-I", "-c",
    "import runpy; assert runpy.run_path('a.py')['main']() == 7"],
    { cwd: dir, encoding: "utf8" }), "");
  git("apply", "--reverse", "change.patch");
  assert.equal(readFileSync(join(dir, "a.py"), "utf8"), source);
  writeFileSync(join(dir, "a.py"), "def leaf():\n    return 99\n");
  assert.throws(() => git("apply", "--check", "change.patch"));
} finally {
  rmSync(dir, { recursive: true, force: true });
  workspace.free();
  restarted.free();
}
console.log("Browser reviews, restart, action coverage and independent patch replay passed.");
