// expect: passes

type Invoice = { readonly number: string; readonly status: string };

function send(invoice: Invoice): Invoice {
  return { number: invoice.number, status: "sent" };
}

function archive(invoiceNumber: string): string {
  return `archived ${invoiceNumber}`;
}

const handlers: Record<string, (...args: any[]) => any> = {
  draft: send,
  sent: archive,
};

export function advance(invoice: Invoice): any {
  return handlers[invoice.status]?.(invoice);
}
