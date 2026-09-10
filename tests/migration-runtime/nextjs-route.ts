export async function GET(): Promise<Response> {
  return Response.json({ service: "telemetry", samples: 3 })
}
