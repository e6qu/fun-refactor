// expect: passes
// Two booleans worth of shape in one type. A settled payment without a
// receipt, and a pending payment with one, both construct without complaint.

type Payment = {
  readonly requestedAt: string;
  readonly settled: boolean;
  readonly receiptId: string | null;
};

export const impossibleA: Payment = { requestedAt: "09:00", settled: true, receiptId: null };
export const impossibleB: Payment = { requestedAt: "09:00", settled: false, receiptId: "r-42" };
