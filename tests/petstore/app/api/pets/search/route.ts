import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"
import * as z from "zod"

import { db } from "@/lib/db"
import { petSearchSchema } from "@/lib/schemas"

export async function POST(req: NextRequest) {
  try {
    const json = await req.json()
    const body = petSearchSchema.parse(json)

    const pets = await db.pet.findMany({
      where: {
        species: body.species,
        ageMonths: { gte: body.minAgeMonths, lte: body.maxAgeMonths },
        tags: { hasEvery: body.tags },
      },
    })

    return NextResponse.json(pets)
  } catch (error) {
    if (error instanceof z.ZodError) {
      return NextResponse.json(error.issues, { status: 422 })
    }
    return new Response(null, { status: 500 })
  }
}
