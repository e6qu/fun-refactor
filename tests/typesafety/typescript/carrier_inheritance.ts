// expect: passes

abstract class Carrier {
  abstract quotePence(kilograms: number): number;
}

class RoyalPost extends Carrier {
  quotePence(kilograms: number): number {
    return Math.floor(kilograms * 120);
  }
}

export function cheapest(carriers: Carrier[], kilograms: number): number {
  return Math.min(...carriers.map((carrier) => carrier.quotePence(kilograms)));
}

export const quote = cheapest([new RoyalPost()], 9.5);
