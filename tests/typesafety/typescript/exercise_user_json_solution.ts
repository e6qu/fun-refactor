// expect: passes
// One parse at the edge. The three checks are gone, because past the parse
// the fields exist and have their types.

import { z } from "zod";

const User = z.object({
  name: z.string(),
  age: z.number().int(),
});

type User = z.infer<typeof User>;

function greeting(user: User): string {
  return `hello ${user.name}`;
}

function canVote(user: User): boolean {
  return user.age >= 18;
}

export function summary(body: string): string {
  const user = User.parse(JSON.parse(body));
  return `${greeting(user)}, can vote: ${canVote(user)}`;
}
