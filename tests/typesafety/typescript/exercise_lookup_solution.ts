// expect: passes

type Ok<T> = { readonly kind: "ok"; readonly value: T };
type Err = { readonly kind: "err"; readonly reason: string };
type Result<T> = Ok<T> | Err;

function ok<T>(value: T): Result<T> {
  return { kind: "ok", value };
}

function err(reason: string): Result<never> {
  return { kind: "err", reason };
}

function andThen<T, U>(result: Result<T>, step: (value: T) => Result<U>): Result<U> {
  return result.kind === "ok" ? step(result.value) : result;
}

function findUserId(login: string): Result<string> {
  return login === "ada" ? ok("u7") : err(`no user for login ${login}`);
}

function findCart(userId: string): Result<string[]> {
  return userId === "u7" ? ok(["book"]) : err(`no cart for ${userId}`);
}

function head(items: string[]): Result<string> {
  const first = items[0];
  return first === undefined ? err("the cart is empty") : ok(first);
}

export function firstItem(login: string): Result<string> {
  return andThen(andThen(findUserId(login), findCart), head);
}
