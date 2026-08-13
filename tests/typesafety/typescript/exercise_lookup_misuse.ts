// expect: fails

type Ok<T> = { readonly kind: "ok"; readonly value: T };
type Err = { readonly kind: "err"; readonly reason: string };
type Result<T> = Ok<T> | Err;

function findUserId(login: string): Result<string> {
  return login === "ada" ? { kind: "ok", value: "u7" } : { kind: "err", reason: `no user for login ${login}` };
}

function greet(userId: string): string {
  return `welcome ${userId}`;
}

export const banner = greet(findUserId("ada"));
