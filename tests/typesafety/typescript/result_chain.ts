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

function parseQuantity(text: string): Result<number> {
  return /^\d+$/.test(text) ? ok(Number(text)) : err(`not a number: ${text}`);
}

function checkStock(quantity: number): Result<number> {
  return quantity <= 10 ? ok(quantity) : err("only 10 in stock");
}

export function quote(text: string): Result<number> {
  return andThen(andThen(parseQuantity(text), checkStock), (q) => ok(q * 250));
}
