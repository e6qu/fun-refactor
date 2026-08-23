import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"
import * as z from "zod"

import { db } from "@/lib/db"
import { petPatchSchema } from "@/lib/schemas"

export async function GET(req: NextRequest, context: { params: { petId: string } }) {
  const pet = await db.pet.findUnique({ where: { id: context.params.petId } })

  if (!pet) {
    return new Response(null, { status: 404 })
  }

  return NextResponse.json(pet)
}

export async function PATCH(req: NextRequest, context: { params: { petId: string } }) {
  try {
    const json = await req.json()
    const body = petPatchSchema.parse(json)

    const pet = await db.pet.update({
      where: { id: context.params.petId },
      data: body,
    })

    return NextResponse.json(pet)
  } catch (error) {
    if (error instanceof z.ZodError) {
      return NextResponse.json(error.issues, { status: 422 })
    }
    return new Response(null, { status: 500 })
  }
}

export async function DELETE(req: NextRequest, context: { params: { petId: string } }) {
  await db.pet.delete({ where: { id: context.params.petId } })
  return new Response(null, { status: 204 })
}
