import { GET } from "./web/app/metrics/[metric_id]/route"

async function main() {
  const response = await GET(new Request("http://local/metrics/7"), {
    params: { metric_id: "7" },
  })
  console.log(JSON.stringify({ status: response.status, body: await response.json() }))
}

main()
