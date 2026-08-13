// expect: fails

type Status = "received" | "picked" | "shipped";

function assertNever(value: never): never {
  throw new Error(`unhandled case: ${JSON.stringify(value)}`);
}

function nextAction(status: Status): string {
  switch (status) {
    case "received":
      return "start picking";
    case "picked":
      return "pack the box";
    case "shipped":
      return "send the tracking mail";
    default:
      return assertNever(status);
  }
}

export function handle(): string {
  return nextAction("recieved"); // rejected: not one of the three statuses
}
