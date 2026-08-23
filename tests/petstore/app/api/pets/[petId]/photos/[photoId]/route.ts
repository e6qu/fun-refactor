import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"

import { db } from "@/lib/db"

export async function GET(
  req: NextRequest,
  context: { params: { petId: string; photoId: string } },
) {
  const photo = await db.photo.findFirst({
    where: { id: context.params.photoId, petId: context.params.petId },
  })

  if (!photo) {
    return new Response(null, { status: 404 })
  }

  return NextResponse.json(photo)
}

export async function DELETE(
  req: NextRequest,
  context: { params: { petId: string; photoId: string } },
) {
  await db.photo.deleteMany({
    where: { id: context.params.photoId, petId: context.params.petId },
  })

  return new Response(null, { status: 204 })
}
