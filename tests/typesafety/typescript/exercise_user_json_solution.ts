// expect: passes

type User = { readonly name: string; readonly age: number };

function parseUser(body: string): User {
  const data: unknown = JSON.parse(body);
  if (
    typeof data !== "object" ||
    data === null ||
    !("name" in data) ||
    !("age" in data) ||
    typeof data.name !== "string" ||
    typeof data.age !== "number"
  ) {
    throw new Error(`not a user: ${body}`);
  }
  return { name: data.name, age: data.age };
}

function greeting(user: User): string {
  return `hello ${user.name}`;
}

function canVote(user: User): boolean {
  return user.age >= 18;
}

export function summary(body: string): string {
  const user = parseUser(body);
  return `${greeting(user)}, can vote: ${canVote(user)}`;
}
