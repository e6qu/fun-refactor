// expect: fails

type DraftInvoice = { readonly stage: "draft"; readonly number: string };
type SentInvoice = { readonly stage: "sent"; readonly number: string };
type PaidInvoice = { readonly stage: "paid"; readonly number: string };

function send(invoice: DraftInvoice): SentInvoice {
  return { stage: "sent", number: invoice.number };
}

function recordPayment(invoice: SentInvoice): PaidInvoice {
  return { stage: "paid", number: invoice.number };
}

export const paid = recordPayment({ stage: "draft", number: "INV-7" });
