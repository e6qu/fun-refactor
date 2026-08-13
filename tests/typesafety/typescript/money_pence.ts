// expect: passes

function totalPence(pricesPence: readonly number[]): number {
  return pricesPence.reduce((sum, price) => sum + price, 0);
}

function show(pence: number): string {
  const shillings = Math.floor(pence / 12);
  const d = pence % 12;
  return `£${Math.floor(shillings / 20)} ${shillings % 20}s ${d}d`;
}

export const sumIsExact = totalPence([24, 24, 24]) === 72;
export const shown = show(72);
