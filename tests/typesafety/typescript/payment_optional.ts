// expect: passes

type Payment = {
  readonly requestedAt: string;
  readonly settled: boolean;
  readonly receiptId: string | null;
};

export const impossibleA: Payment = { requestedAt: "09:00", settled: true, receiptId: null };
export const impossibleB: Payment = { requestedAt: "09:00", settled: false, receiptId: "r-42" };
