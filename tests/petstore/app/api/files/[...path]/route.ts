import type { NextRequest } from "next/server"

import { storage } from "@/lib/storage"

export async function GET(req: NextRequest, context: { params: { path: string[] } }) {
  const object = await storage.get(context.params.path.join("/"))

  if (!object) {
    return new Response(null, { status: 404 })
  }

  return new Response(object.body, {
    status: 200,
    headers: { "content-type": object.contentType },
  })
}
