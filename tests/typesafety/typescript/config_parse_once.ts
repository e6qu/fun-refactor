// expect: passes

type Config = {
  readonly port: number;
  readonly verbose: boolean;
};

export function parseArgv(argv: string[]): Config {
  const settings = new Map<string, string>();
  for (const pair of argv) {
    const [key, value] = pair.split("=", 2);
    settings.set(key ?? "", value ?? "");
  }
  const portText = settings.get("port");
  if (portText === undefined || !/^\d+$/.test(portText)) {
    throw new Error("port missing or not a number");
  }
  const port = Number(portText);
  if (port < 1 || port > 65535) {
    throw new Error("port out of range");
  }
  const verboseText = settings.get("verbose") ?? "false";
  if (verboseText !== "true" && verboseText !== "false") {
    throw new Error("verbose must be true or false");
  }
  return { port, verbose: verboseText === "true" };
}

export function connect(config: Config): string {
  return `connecting on ${config.port}`;
}

export function report(config: Config): string {
  return `listening on ${config.port}`;
}
