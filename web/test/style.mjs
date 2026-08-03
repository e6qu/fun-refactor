/**
 * The stylesheet names no colours, and both themes name the same ones.
 *
 * Two failures hide from whoever writes them, because a stylesheet is only ever
 * looked at in one theme at a time:
 *
 *   - a colour written literally works in whichever theme it was picked for and is
 *     invisible in the other — grey-on-grey, or black text on a black panel;
 *   - a token declared for light and forgotten for dark falls back to the light
 *     value, so one element stays bright in a dark page.
 *
 * Neither shows up in a build, a type check or a screenshot of the theme you happen
 * to be in. So they are checked.
 *
 *     node web/test/style.mjs
 */

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const css = readFileSync(join(here, "../src/style.css"), "utf8");

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

/** The blocks that are allowed to name colours: they are the definitions. */
function tokenBlocks(source) {
  const blocks = [];
  const pattern = /(:root[^{]*)\{([^}]*)\}/g;
  let match;
  while ((match = pattern.exec(source)) !== null) {
    blocks.push({ selector: match[1].trim(), body: match[2], at: match.index, end: pattern.lastIndex });
  }
  return blocks;
}

const blocks = tokenBlocks(css);

console.log("style");

check("the token blocks exist", () => {
  const selectors = blocks.map((b) => b.selector);
  assert(blocks.length >= 3, `expected light, system-dark and explicit-dark, got ${selectors}`);
  return selectors.join(" · ");
});

check("no colour is written outside a token block", () => {
  // Blank out the token blocks, then look for anything colour-shaped in what is left.
  let rest = css;
  for (const block of [...blocks].reverse()) {
    rest = rest.slice(0, block.at) + " ".repeat(block.end - block.at) + rest.slice(block.end);
  }
  // Comments explain the palette; they are prose, not style.
  rest = rest.replace(/\/\*[\s\S]*?\*\//g, "");

  const offences = [];
  const colour = /#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(|\b(?:white|black|silver|gray|grey|red|blue|green|yellow|orange|purple|pink|brown)\b(?!-)/g;
  for (const [index, line] of rest.split("\n").entries()) {
    // `currentColor` is not a colour, it is a reference to one.
    const cleaned = line.replace(/currentColor/g, "");
    const found = cleaned.match(colour);
    if (found) offences.push(`  style.css:${index + 1}: ${found.join(", ")} — ${line.trim()}`);
  }
  assert(
    offences.length === 0,
    `a literal colour works in one theme and is invisible in the other:\n${offences.join("\n")}`,
  );
  return "";
});

check("every token light declares, dark declares too", () => {
  const names = (body) => new Set([...body.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));

  const light = blocks.find((b) => b.selector === ":root");
  assert(light, "no plain `:root` block");
  const dark = blocks.filter((b) => b.selector.includes('data-theme="dark"'));
  assert(dark.length > 0, "no explicit dark block");

  const lightNames = names(light.body);
  // Fonts and radii are not theme-dependent, and repeating them would only be a
  // second place to change them.
  const themed = [...lightNames].filter(
    (n) => !["--mono", "--sans", "--radius", "--radius-lg"].includes(n),
  );
  const darkNames = new Set(dark.flatMap((b) => [...names(b.body)]));

  const missing = themed.filter((n) => !darkNames.has(n));
  assert(
    missing.length === 0,
    `these fall back to their light value in dark mode: ${missing.join(", ")}`,
  );
  return `${themed.length} themed tokens`;
});

check("the system-dark and explicit-dark blocks agree", () => {
  const systemDark = blocks.find((b) => b.selector.includes(':not([data-theme="light"])'));
  const explicitDark = blocks.find((b) => b.selector.includes('data-theme="dark"'));
  assert(systemDark && explicitDark, "expected both a system-dark and an explicit-dark block");

  const pairs = (body) => {
    const out = new Map();
    for (const m of body.matchAll(/(--[a-z0-9-]+)\s*:\s*([^;]+);/g)) out.set(m[1], m[2].trim());
    return out;
  };
  const a = pairs(systemDark.body);
  const b = pairs(explicitDark.body);

  const differing = [...a.keys()].filter((k) => b.get(k) !== a.get(k));
  assert(
    differing.length === 0,
    `the two dark blocks disagree, so the theme changes when you pin the one you are ` +
      `already in: ${differing.join(", ")}`,
  );
  return `${a.size} tokens, identical`;
});

check("every variable used is defined", () => {
  const defined = new Set(blocks.flatMap((b) => [...b.body.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1])));
  const used = new Set([...css.matchAll(/var\((--[a-z0-9-]+)/g)].map((m) => m[1]));
  const undef = [...used].filter((n) => !defined.has(n));
  assert(undef.length === 0, `used but never defined: ${undef.join(", ")}`);
  return `${used.size} used, ${defined.size} defined`;
});

console.log(`\n${checks - failures}/${checks} passed`);
process.exit(failures === 0 ? 0 : 1);
