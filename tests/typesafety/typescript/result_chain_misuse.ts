// expect: fails

type Ok<T> = { readonly kind: "ok"; readonly value: T };
type Err = { readonly kind: "err"; readonly reason: string };
type Result<T> = Ok<T> | Err;

function quote(text: string): Result<number> {
  return /^\d+$/.test(text)
    ? { kind: "ok", value: Number(text) * 250 }
    : { kind: "err", reason: `not a number: ${text}` };
}

export function totalWithShipping(text: string): number {
  return quote(text) + 45;
}
