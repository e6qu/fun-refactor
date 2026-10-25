// Same spelling, different declaration. Preserve every byte here.
export function price(amount: number): number {
    return amount + 100;
}

export function localReceipt(amount: number): number {
    return price(amount);
}
