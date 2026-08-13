// expect: passes

type Invoice = {
  number: string;
  status: string;
};

function send(invoice: Invoice): void {
  if (invoice.status !== "draft") {
    throw new Error("only a draft can be sent");
  }
  invoice.status = "sent";
}

function recordPayment(invoice: Invoice): void {
  if (invoice.status !== "sent") {
    throw new Error("only a sent invoice can be paid");
  }
  invoice.status = "paid";
}

export function rush(invoice: Invoice): void {
  recordPayment(invoice);
  send(invoice);
}
