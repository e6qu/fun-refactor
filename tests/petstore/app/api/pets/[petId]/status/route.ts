import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"
import * as z from "zod"

import { db } from "@/lib/db"
import { statusUpdateSchema } from "@/lib/schemas"

export async function PUT(req: NextRequest, context: { params: { petId: string } }) {
  try {
    const json = await req.json()
    const body = statusUpdateSchema.parse(json)

    const pet = await db.pet.update({
      where: { id: context.params.petId },
      data: { status: body.status, statusNote: body.note },
    })

    return NextResponse.json(pet)
  } catch (error) {
    if (error instanceof z.ZodError) {
      return NextResponse.json(error.issues, { status: 422 })
    }
    return new Response(null, { status: 500 })
  }
}
