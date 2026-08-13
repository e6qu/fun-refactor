// expect: passes

function shelfLabel(item: string, price: number): string {
  return `${item}: ${price.toFixed(2)} EUR`;
}

export const label: string = shelfLabel("tea", 4.5);
