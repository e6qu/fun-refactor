// expect: fails

declare const penceBrand: unique symbol;
declare const rateBrand: unique symbol;

type Pence = number & { readonly [penceBrand]: true };
type Rate = number & { readonly [rateBrand]: true };

function pence(n: number): Pence {
  return n as Pence;
}

function rate(n: number): Rate {
  return n as Rate;
}

function applyDiscount(total: Pence, discount: Rate): Pence {
  return pence(Math.round(total * (1 - discount)));
}

export const discounted = applyDiscount(rate(0.1), pence(1250));
