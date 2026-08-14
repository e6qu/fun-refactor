// expect: passes

type Status = "draft" | "sent" | "paid";

type Invoice = { readonly number: string; readonly status: Status };

function send(invoice: Invoice): Invoice {
  return { number: invoice.number, status: "sent" };
}

function recordPayment(invoice: Invoice): Invoice {
  return { number: invoice.number, status: "paid" };
}

function keep(invoice: Invoice): Invoice {
  return invoice;
}

const handlers: Record<Status, (invoice: Invoice) => Invoice> = {
  draft: send,
  sent: recordPayment,
  paid: keep,
};

export function advance(invoice: Invoice): Invoice {
  return handlers[invoice.status](invoice);
}
