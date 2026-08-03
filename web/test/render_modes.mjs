/**
 * The machine-readable renderings describe the interface that actually exists.
 *
 * `?render_as=json` is a promise that the JSON says what a person would be looking
 * at. Nothing enforces that by construction — the view description is written by
 * hand — so the way it rots is silent: an action is added to the toolbar and the JSON
 * keeps describing yesterday's menu, and the caller relying on it never sees a
 * failure, only an answer that is wrong.
 *
 * This reads both files and checks they still agree about what exists.
 *
 *     node web/test/render_modes.mjs
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const src = (name) => readFileSync(join(here, "../src", name), "utf8");

const actions = src("actions.ts");
const serialise = src("serialise.ts");
const app = src("app.ts");
const main = src("main.ts");

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

/** The group names `actions.ts` declares. */
function groups() {
  const block = actions.match(/export const GROUPS = \[([\s\S]*?)\] as const;/);
  if (!block) throw new Error("actions.ts no longer declares GROUPS");
  return [...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
}

console.log("render modes");

check("every mode the dispatcher accepts is handled", () => {
  const declared = main.match(/const MODES = \[([\s\S]*?)\] as const;/);
  assert(declared, "main.ts no longer declares MODES");
  const modes = [...declared[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  assert(modes.includes("html"), "html must remain a mode, and the default");
  for (const mode of modes.filter((m) => m !== "html")) {
    assert(
      serialise.includes(`"${mode}"`),
      `serialise.ts never mentions '${mode}', so asking for it renders the wrong thing`,
    );
  }
  return modes.join(", ");
});

check("the json view describes every action group", () => {
  // Each group must appear either in a named toolbar menu or in the context-menu
  // list, or the description is of a narrower interface than the page has.
  const described = serialise.match(/menus: \[([\s\S]*?)\],\n\s+\/\/ The same list/);
  assert(described, "serialise.ts no longer builds a `menus` list");
  const missing = groups().filter(
    (g) => g !== "Navigate" && !described[1].includes(`"${g}"`),
  );
  assert(
    missing.length === 0,
    `these groups exist in actions.ts and no toolbar menu offers them: ${missing.join(", ")}`,
  );
  return `${groups().length} groups`;
});

check("the json view and the page agree on the toolbar menus", () => {
  // Both build their menus from group names; if they name different ones, the JSON
  // is describing a toolbar the page does not draw.
  const inApp = [...app.matchAll(/dropdown\(el<HTMLButtonElement>\("([a-z]+)-menu"\), \[([\s\S]*?)\]\)/g)];
  assert(inApp.length >= 2, "app.ts no longer builds toolbar dropdowns");

  for (const [, name, body] of inApp) {
    const wanted = [...body.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    const label = name[0].toUpperCase() + name.slice(1);
    const described = serialise.match(
      new RegExp(`label: "${label}",\\s+items: actionsIn\\(\\[([\\s\\S]*?)\\]\\)`),
    );
    assert(described, `the JSON view has no '${label}' menu, but the page draws one`);
    const has = [...described[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);
    assert(
      JSON.stringify(has) === JSON.stringify(wanted),
      `'${label}' offers ${JSON.stringify(wanted)} in the page and ` +
        `${JSON.stringify(has)} in the JSON`,
    );
  }
  return `${inApp.length} menus`;
});

check("json_data carries no layout", () => {
  const body = serialise.match(/function data\(([\s\S]*?)\n}/);
  assert(body, "serialise.ts no longer has a `data` function");
  for (const layout of ["panes:", "toolbar:", "editor:", "menus:"]) {
    assert(
      !body[1].includes(layout),
      `json_data includes '${layout}' — it is meant to be the data without the view`,
    );
  }
  return "";
});

check("the app is never imported by the machine-readable path", () => {
  // The whole point of splitting them: a caller asking for JSON should not fetch
  // four megabytes of editor to get it.
  assert(
    !serialise.includes('from "./app"') && !serialise.includes('import("./app")'),
    "serialise.ts imports the app, which pulls in Monaco",
  );
  assert(
    !serialise.includes("monaco"),
    "serialise.ts references monaco",
  );
  return "";
});

console.log(`\n${checks - failures}/${checks} passed`);
process.exit(failures === 0 ? 0 : 1);
