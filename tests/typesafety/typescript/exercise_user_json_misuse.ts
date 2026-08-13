// expect: fails

type User = { readonly name: string; readonly age: number };

function greeting(user: User): string {
  return `hello ${user.name}`;
}

export const hello = greeting({ name: "Ada", age: "36" });
