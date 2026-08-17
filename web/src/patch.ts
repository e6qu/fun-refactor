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

/**
 * A file's lines, without the phantom line a trailing newline splits off.
 *
 * `"a\nb\n".split("\n")` ends with an empty string that is not a line of the
 * file. Counted as one, a hunk that reached the end of the file claimed one
 * more old line than the file has, and `git apply` refused the whole patch.
 * The flag says whether the text ended with a newline, which the diff has to
 * state when it did not.
 */
function linesOf(text: string): [string[], boolean] {
  const terminated = text.endsWith("\n");
  const lines = text.split("\n");
  if (terminated) lines.pop();
  return [lines, terminated];
}

const NO_NEWLINE = "\\ No newline at end of file";

/** A unified diff for one file. Empty when the two texts are identical. */
export function diffOf(path: string, before: string, after: string): string {
  if (before === after) return "";

  const [a, aTerminated] = linesOf(before);
  const [b, bTerminated] = linesOf(after);

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

  // The marker goes right after the line that is the file's last, per side.
  // With shared trailing context both sides end on the same line, so one marker
  // after the context covers both; a hunk that rewrites the last line marks the
  // removed and the added halves separately.
  const reachesEnd = tail <= CONTEXT;
  const removedLines = removed.map((l) => `-${l}`);
  const addedLines = added.map((l) => `+${l}`);
  const trailLines = trail.map((l) => ` ${l}`);
  if (reachesEnd && trail.length > 0) {
    if (!aTerminated || !bTerminated) trailLines.push(NO_NEWLINE);
  } else if (reachesEnd && trail.length === 0) {
    if (!aTerminated && removedLines.length > 0) removedLines.push(NO_NEWLINE);
    if (!bTerminated && addedLines.length > 0) addedLines.push(NO_NEWLINE);
  }

  const lines = [
    `--- a/${path}`,
    `+++ b/${path}`,
    `@@ -${from + 1},${oldCount} +${from + 1},${newCount} @@`,
    ...lead.map((l) => ` ${l}`),
    ...removedLines,
    ...addedLines,
    ...trailLines,
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
