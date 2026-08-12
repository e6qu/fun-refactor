// expect: passes
// A union of string literals lists every value the type allows. The checker
// rejects the rest.

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
