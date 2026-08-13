// expect: passes

function readArgv(argv: string[]): Map<string, string> {
  const settings = new Map<string, string>();
  for (const pair of argv) {
    const [key, value] = pair.split("=", 2);
    settings.set(key ?? "", value ?? "");
  }
  return settings;
}

export const settings = readArgv(["port=8080", "verbose=true"]);

export function connect(settings: Map<string, string>): string {
  const portText = settings.get("port");
  if (portText === undefined || !/^\d+$/.test(portText)) {
    throw new Error("port missing or not a number");
  }
  return `connecting on ${Number(portText)}`;
}

export function report(settings: Map<string, string>): string {
  const portText = settings.get("port");
  if (portText === undefined || !/^\d+$/.test(portText)) {
    throw new Error("port missing or not a number");
  }
  return `listening on ${Number(portText)}`;
}
