// expect: passes

export function readLog(path: string, mode: "text" | "binary"): number {
  const recordSize = mode === "text" ? 1 : 8;
  return path.length * recordSize;
}

export function tail(): number {
  return readLog("app.log", "binary");
}
