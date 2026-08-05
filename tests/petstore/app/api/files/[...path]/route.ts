import type { NextRequest } from "next/server"

import { storage } from "@/lib/storage"

/**
 * A stored file, reached by its whole path.
 *
 * `[...path]` is a catch-all: it matches across slashes, so `/files/pets/7/front.jpg`
 * arrives here as one parameter. FastAPI spells that `{path:path}`, and a translation
 * that emitted `{path}` would answer a strictly smaller set of URLs than the one it
 * replaced — silently, and only for the requests with a slash in them.
 */
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
