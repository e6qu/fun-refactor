// expect: fails

type Currency = "USD" | "EUR";

type Money<C extends Currency> = {
  readonly currency: C;
  readonly cents: number;
};

function money<C extends Currency>(currency: C, cents: number): Money<C> {
  return { currency, cents };
}

function add<C extends Currency>(a: Money<C>, b: Money<NoInfer<C>>): Money<C> {
  return { currency: a.currency, cents: a.cents + b.cents };
}

export function basketTotal(): Money<"USD"> {
  return add(money("USD", 1999), money("EUR", 500));
}
