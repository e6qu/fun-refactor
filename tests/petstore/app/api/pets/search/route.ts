import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"
import * as z from "zod"

import { db } from "@/lib/db"
import { petSearchSchema } from "@/lib/schemas"

/**
 * Search, which is a POST because the query is a body.
 *
 * Not every endpoint is CRUD, and `search` is a *sibling* of `[petId]` in the tree —
 * so `/pets/search` and `/pets/{pet_id}` are two different URLs that a router has to
 * tell apart. The order they are declared in decides which one wins.
 */
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
