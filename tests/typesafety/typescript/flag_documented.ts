// expect: passes
// The comment says two values are allowed. Nothing enforces it, so any string
// arrives here.

/** mode is "text" or "binary". */
function readLog(path: string, mode: string): number {
  const recordSize = mode === "text" ? 1 : 8;
  return path.length * recordSize;
}

export function tail(): number {
  return readLog("app.log", "binry"); // typo: silently reads as binary
}
