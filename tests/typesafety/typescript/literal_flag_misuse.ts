// expect: fails
// A value typed `string` could be anything, so the checker refuses to pass it
// where only two values are allowed. Type the variable as the literal, or pass
// the value directly.

function readLog(path: string, mode: "text" | "binary"): number {
  const recordSize = mode === "text" ? 1 : 8;
  return path.length * recordSize;
}

export function tail(chosen: string): number {
  return readLog("app.log", chosen); // error: string is wider than the two values
}
