// expect: passes

/** mode is "text" or "binary". */
function readLog(path: string, mode: string): number {
  const recordSize = mode === "text" ? 1 : 8;
  return path.length * recordSize;
}

export function tail(): number {
  return readLog("app.log", "binry");
}
