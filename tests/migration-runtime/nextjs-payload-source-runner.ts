import { POST } from "./app/readings/route"

async function main() {
  const payload = {
    sensor_id: "sensor-4",
    measuredAt: "2026-09-10T10:30:00Z",
    values: [3.25, 5.5, 8.75],
  }
  const response = await POST(new Request("http://local/readings", {
    method: "POST",
    body: JSON.stringify(payload),
  }))
  console.log(JSON.stringify({ status: response.status, body: await response.json() }))
}

main()
