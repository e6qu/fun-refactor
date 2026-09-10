import { POST } from "./web/app/events/route"

async function main() {
  const payload = {
    event_id: "evt-9",
    sentAt: 1757500200,
    labels: ["accepted", "priority"],
  }
  const response = await POST(new Request("http://local/events", {
    method: "POST",
    body: JSON.stringify(payload),
  }), { params: {} })
  console.log(JSON.stringify({ status: response.status, body: await response.json() }))
}

main()
