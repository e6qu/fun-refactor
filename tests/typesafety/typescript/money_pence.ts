// expect: passes

function totalPence(pricesPence: number[]): number {
  return pricesPence.reduce((sum, price) => sum + price, 0);
}

function show(pence: number): string {
  const shillings = Math.floor(pence / 12);
  const d = pence % 12;
  return `£${Math.floor(shillings / 20)} ${shillings % 20}s ${d}d`;
}

// The same three parts: two shillings is 24 pence.
export const sumIsExact = totalPence([24, 24, 24]) === 72; // true
export const shown = show(72); // "£0 6s 0d"
