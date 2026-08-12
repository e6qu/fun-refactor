// expect: passes
// The docstring used to say "mode is 'text' or 'binary'". A literal union moves
// that sentence into the signature, where the checker reads it.

export function readLog(path: string, mode: "text" | "binary"): number {
  const recordSize = mode === "text" ? 1 : 8;
  return path.length * recordSize;
}

export function tail(): number {
  return readLog("app.log", "binary");
}
