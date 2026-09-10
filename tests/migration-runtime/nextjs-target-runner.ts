import { GET } from "./web/app/metrics/[metricId]/route"

async function main() {
  const response = await GET(new Request("http://local/metrics/7"), {
    params: { metricId: "7" },
  })
  console.log(JSON.stringify({ status: response.status, body: await response.json() }))
}

main()
