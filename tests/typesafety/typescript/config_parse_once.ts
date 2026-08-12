// expect: passes
// One function turns the argument strings into a Config, or throws. Past that
// point `port` exists and is a number, and the type says so.

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
  return { port: Number(portText), verbose: settings.get("verbose") === "true" };
}

export function connect(config: Config): string {
  return `connecting on ${config.port}`; // no check, and none needed
}

export function report(config: Config): string {
  return `listening on ${config.port}`;
}
