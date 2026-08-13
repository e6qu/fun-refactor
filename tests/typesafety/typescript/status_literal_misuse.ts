// expect: fails

type Status = "draft" | "sent" | "paid";

function advance(status: Status): Status {
  switch (status) {
    case "draft":
      return "sent";
    case "sent":
      return "paid";
    case "paid":
      return "paid";
  }
}

export function submit(): Status {
  return advance("snet"); // error: not one of the three statuses
}
