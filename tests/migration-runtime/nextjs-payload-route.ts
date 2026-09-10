interface ReadingEnvelope {
  sensor_id: string
  measuredAt: string
  values: number[]
}

export async function POST(request: Request): Promise<Response> {
  const reading: ReadingEnvelope = await request.json()
  return Response.json({
    sensor_id: reading.sensor_id,
    measuredAt: reading.measuredAt,
    values: reading.values,
  })
}
