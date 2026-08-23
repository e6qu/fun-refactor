import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"

import { db } from "@/lib/db"

export async function GET(req: NextRequest, context: { params: { storeId: string } }) {
  const counts = await db.pet.groupBy({
    by: ["species"],
    where: { storeId: context.params.storeId },
    _count: true,
  })

  return NextResponse.json(counts)
}
