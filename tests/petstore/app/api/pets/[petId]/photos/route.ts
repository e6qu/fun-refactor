import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"
import * as z from "zod"

import { db } from "@/lib/db"
import { photoCreateSchema } from "@/lib/schemas"

export async function GET(req: NextRequest, context: { params: { petId: string } }) {
  const photos = await db.photo.findMany({
    where: { petId: context.params.petId },
    orderBy: { takenAt: "desc" },
  })

  return NextResponse.json(photos)
}

export async function POST(req: NextRequest, context: { params: { petId: string } }) {
  try {
    const json = await req.json()
    const body = photoCreateSchema.parse(json)

    const photo = await db.photo.create({
      data: {
        petId: context.params.petId,
        url: body.url,
        caption: body.caption,
        widthPx: body.widthPx,
        heightPx: body.heightPx,
      },
    })

    return NextResponse.json(photo, { status: 201 })
  } catch (error) {
    if (error instanceof z.ZodError) {
      return NextResponse.json(error.issues, { status: 422 })
    }
    return new Response(null, { status: 500 })
  }
}
