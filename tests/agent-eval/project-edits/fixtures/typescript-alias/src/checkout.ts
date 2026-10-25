import { price as displayPrice } from './pricing';

// The local alias is part of this consumer's interface.
export function receipt(amount: number): number {
    return displayPrice(amount);
}
