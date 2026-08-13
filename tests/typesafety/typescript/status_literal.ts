// expect: passes

type Status = "draft" | "sent" | "paid";

export function advance(status: Status): Status {
  switch (status) {
    case "draft":
      return "sent";
    case "sent":
      return "paid";
    case "paid":
      return "paid";
  }
}
