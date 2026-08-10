/**
 * A unified diff of the whole session, for pasting into a pull request.
 *
 * Whole-file and not per-hunk: the analysis already prints proper hunks for each
 * refactoring, and what this has to produce is one patch a person can read and
 * `git apply` will take. A common prefix and suffix are trimmed so an edit deep in a
 * large file does not print the file.
 *
 * Split out from the page because a patch that does not apply is a silent lie — the
 * button downloads something, it just does not work — and the only way to know is to
 * run `git apply` on real output. `web/test/patch.mjs` does that.
 */

const CONTEXT = 3;

/** A unified diff for one file. Empty when the two texts are identical. */
export function diffOf(path: string, before: string, after: string): string {
  if (before === after) return "";

  const a = before.split("\n");
  const b = after.split("\n");

  let head = 0;
  while (head < a.length && head < b.length && a[head] === b[head]) head += 1;

  let tail = 0;
  while (
    tail < a.length - head &&
    tail < b.length - head &&
    a[a.length - 1 - tail] === b[b.length - 1 - tail]
  ) {
    tail += 1;
  }

  const removed = a.slice(head, a.length - tail);
  const added = b.slice(head, b.length - tail);
  const from = Math.max(0, head - CONTEXT);
  const lead = a.slice(from, head);
  const trail = a.slice(a.length - tail, a.length - tail + CONTEXT);

  // Hunk counts are lines *of each side*, context included. Getting these wrong is
  // how a patch that looks right is rejected with "corrupt patch at line N".
  const oldCount = lead.length + removed.length + trail.length;
  const newCount = lead.length + added.length + trail.length;

  const lines = [
    `--- a/${path}`,
    `+++ b/${path}`,
    `@@ -${from + 1},${oldCount} +${from + 1},${newCount} @@`,
    ...lead.map((l) => ` ${l}`),
    ...removed.map((l) => `-${l}`),
    ...added.map((l) => `+${l}`),
    ...trail.map((l) => ` ${l}`),
  ];
  return lines.join("\n") + "\n";
}

/** Every changed file, as one patch. */
export function patchOf(
  paths: string[],
  before: Record<string, string>,
  after: Record<string, string>,
): string {
  return paths
    .map((path) => diffOf(path, before[path] ?? "", after[path] ?? ""))
    .filter(Boolean)
    .join("");
}
