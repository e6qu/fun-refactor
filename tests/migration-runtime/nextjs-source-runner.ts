import { GET } from "./app/internal/telemetry/route"

async function main() {
  const response = await GET()
  console.log(JSON.stringify({ status: response.status, body: await response.json() }))
}

main()
