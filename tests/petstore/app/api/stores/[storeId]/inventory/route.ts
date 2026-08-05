import { NextResponse } from "next/server"
import type { NextRequest } from "next/server"

import { db } from "@/lib/db"

/** How many of each species one store holds. An aggregate, not a resource. */
export async function GET(req: NextRequest, context: { params: { storeId: string } }) {
  const counts = await db.pet.groupBy({
    by: ["species"],
    where: { storeId: context.params.storeId },
    _count: true,
  })

  return NextResponse.json(counts)
}
