// expect: fails

type Status = "draft" | "sent" | "paid";

type Invoice = { readonly number: string; readonly status: Status };

function send(invoice: Invoice): Invoice {
  return { number: invoice.number, status: "sent" };
}

function archive(invoiceNumber: string): string {
  return `archived ${invoiceNumber}`;
}

const handlers: Record<Status, (invoice: Invoice) => Invoice> = {
  draft: send,
  sent: archive,
  paid: send,
};
