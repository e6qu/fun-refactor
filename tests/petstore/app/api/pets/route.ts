import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"
import * as z from "zod"

import { db } from "@/lib/db"
import { petCreateSchema } from "@/lib/schemas"

/** Every pet, newest first. */
export async function GET(req: NextRequest) {
  const limit = Number(req.nextUrl.searchParams.get("limit") ?? "50")
  const species = req.nextUrl.searchParams.get("species")

  const pets = await db.pet.findMany({
    where: species ? { species } : {},
    take: limit,
    orderBy: { arrivedAt: "desc" },
  })

  return NextResponse.json(pets)
}

/** Take in a new pet. */
export async function POST(req: NextRequest) {
  try {
    const json = await req.json()
    const body = petCreateSchema.parse(json)

    const pet = await db.pet.create({
      data: {
        name: body.name,
        species: body.species,
        ageMonths: body.ageMonths,
        tags: body.tags,
        microchipId: body.microchipId,
        arrivedAt: body.arrivedAt,
      },
    })

    return NextResponse.json(pet, { status: 201 })
  } catch (error) {
    if (error instanceof z.ZodError) {
      return NextResponse.json(error.issues, { status: 422 })
    }
    return new Response(null, { status: 500 })
  }
}
