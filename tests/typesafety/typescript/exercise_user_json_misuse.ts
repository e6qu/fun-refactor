// expect: fails

import { z } from "zod";

const User = z.strictObject({
  name: z.string(),
  age: z.number().int(),
});

type User = z.infer<typeof User>;

function greeting(user: User): string {
  return `hello ${user.name}`;
}

export const hello = greeting({ name: "Ada", age: "36" });
