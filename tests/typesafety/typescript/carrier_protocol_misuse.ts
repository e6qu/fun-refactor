// expect: fails

type Carrier = {
  quotePence(kilograms: number): number;
};

class ChattyCourier {
  quotePence(kilograms: number): string {
    return `about ${Math.floor(kilograms) * 100} pence`;
  }
}

function cheapest(carriers: readonly Carrier[], kilograms: number): number {
  return Math.min(...carriers.map((carrier) => carrier.quotePence(kilograms)));
}

export const best = cheapest([new ChattyCourier()], 9.5);
