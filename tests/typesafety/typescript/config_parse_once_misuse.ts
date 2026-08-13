// expect: fails

type Config = {
  readonly port: number;
  readonly verbose: boolean;
};

function connect(config: Config): string {
  return `connecting on ${config.port}`;
}

export const greeting = connect(new Map([["port", "8080"], ["verbose", "true"]]));
