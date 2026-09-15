/** Check checkpoint replacement and recovery without a browser. */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const { loadCheckpoint, saveCheckpoint } = await import(new URL("../src/session.ts", import.meta.url));
const wasm = await import(join(here, "../src/wasm/fun_refactor.js"));
await wasm.default({
  module_or_path: readFileSync(join(here, "../src/wasm/fun_refactor_bg.wasm")),
});

class MemoryStorage {
  values = new Map();
  getItem(key) { return this.values.get(key) ?? null; }
  setItem(key, value) { this.values.set(key, value); }
  removeItem(key) { this.values.delete(key); }
}

let failures = 0;
let checks = 0;
function check(name, run) {
  checks += 1;
  try {
    run();
    console.log(`  ok   ${name}`);
  } catch (error) {
    failures += 1;
    console.log(`  FAIL ${name}: ${error.message}`);
  }
}
function assert(condition, message) {
  if (!condition) throw new Error(message);
}

console.log("browser checkpoint storage");
check("saves and restores one exact envelope", () => {
  const storage = new MemoryStorage();
  const state = JSON.stringify({ body: 1 });
  assert(saveCheckpoint(storage, { session: () => state }, "demo") === null, "save failed");
  const recovered = loadCheckpoint(storage, (value) => ({ value }));
  assert(recovered.error === null, recovered.error);
  assert(recovered.checkpoint.name === "demo", "name changed");
  assert(recovered.checkpoint.workspace.value === state, "session changed");
});

check("does not replace a valid checkpoint when export refuses", () => {
  const storage = new MemoryStorage();
  const state = JSON.stringify({ body: 1 });
  saveCheckpoint(storage, { session: () => state }, "first");
  const before = [...storage.values.values()][0];
  const error = saveCheckpoint(storage, { session: () => '{"error":"too large"}' }, "second");
  assert(error === "too large", "wrong refusal");
  assert([...storage.values.values()][0] === before, "valid checkpoint was replaced");
});

check("discards an invalid envelope", () => {
  const storage = new MemoryStorage();
  storage.setItem("fr-playground-checkpoint-1", '{"schema":"wrong"}');
  const recovered = loadCheckpoint(storage, () => ({}));
  assert(recovered.checkpoint === null && recovered.error, "bad envelope was accepted");
  assert(storage.values.size === 0, "bad envelope was retained");
});

check("reports quota failure and keeps the prior checkpoint", () => {
  const storage = new MemoryStorage();
  saveCheckpoint(storage, { session: () => "{}" }, "first");
  const before = [...storage.values.values()][0];
  storage.setItem = () => { throw new Error("quota"); };
  const error = saveCheckpoint(storage, { session: () => "{}" }, "second");
  assert(error.includes("quota"), "quota error was hidden");
  assert([...storage.values.values()][0] === before, "prior checkpoint was lost");
});

check("restores real wasm files and the next redo operation", () => {
  const workspace = new wasm.Workspace({
    "a.py": "def add(x: int) -> int:\n    return x\n",
  });
  const first = JSON.parse(workspace.rename("a.py", 1, 5, "sum_one"));
  assert(first.transaction === 1, "rename did not create a transaction");
  assert(JSON.parse(workspace.undo(1)).status === "undone", "undo failed");
  const restored = wasm.Workspace.from_session(workspace.session());
  assert(restored.read("a.py").includes("def add"), "restored the wrong file state");
  assert(JSON.parse(restored.redo(1)).status === "applied", "restored redo failed");
  assert(restored.read("a.py").includes("sum_one"), "redo did not restore the edit");
  restored.free();
  workspace.free();
});

check("compacts old wasm steps without changing the cumulative patch", () => {
  const workspace = new wasm.Workspace({
    "a.py": "def add(x: int) -> int:\n    return x\n",
  });
  workspace.rename("a.py", 1, 5, "sum_one");
  workspace.rename("a.py", 1, 5, "sum_two");
  const patch = workspace.patch();
  const compacted = JSON.parse(workspace.compact_history(1));
  assert(compacted.frozen_transactions === 1, "wrong frozen transaction count");
  assert(workspace.patch() === patch, "compaction changed the cumulative patch");
  assert(JSON.parse(workspace.undo(2)).status === "undone", "retained undo failed");
  workspace.free();
});

check("the real wasm restoration gate rejects changed session bytes", () => {
  const workspace = new wasm.Workspace({ "a.py": "x = 1\n" });
  const changed = workspace.session().replace("x = 1", "x = 2");
  let refused = false;
  try {
    wasm.Workspace.from_session(changed);
  } catch (error) {
    refused = String(error).includes("digest");
  }
  workspace.free();
  assert(refused, "changed session was accepted");
});

console.log(`\n${checks - failures}/${checks} passed`);
process.exit(failures === 0 ? 0 : 1);
