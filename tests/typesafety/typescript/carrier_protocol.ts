// expect: passes

type Carrier = {
  quotePence(kilograms: number): number;
};

class RoyalPost {
  quotePence(kilograms: number): number {
    return Math.floor(kilograms * 120);
  }
}

const villageCourier: Carrier = {
  quotePence: () => 90,
};

export function cheapest(carriers: readonly Carrier[], kilograms: number): number {
  return Math.min(...carriers.map((carrier) => carrier.quotePence(kilograms)));
}

export const quote = cheapest([new RoyalPost(), villageCourier], 9.5);
