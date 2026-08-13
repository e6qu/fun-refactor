// expect: passes

function greeting(user: Record<string, unknown>): string {
  const name = user["name"];
  if (typeof name !== "string") {
    throw new Error("name missing");
  }
  return `hello ${name}`;
}

function canVote(user: Record<string, unknown>): boolean {
  const age = user["age"];
  if (typeof age !== "number") {
    throw new Error("age missing");
  }
  return age >= 18;
}

export function summary(body: string): string {
  const user: unknown = JSON.parse(body);
  if (typeof user !== "object" || user === null) {
    throw new Error("not an object");
  }
  return `${greeting(user as Record<string, unknown>)}, can vote: ${canVote(user as Record<string, unknown>)}`;
}
