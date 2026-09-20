import { z } from "zod";
const Query = z.object({limit: z.coerce.number().int()});
const Body = z.object({visible: z.boolean()});
export async function POST(request: Request, context: {params: Promise<{id: string}>}) {
  const params = await context.params;
  const parsed = Query.safeParse(Object.fromEntries(new URL(request.url).searchParams));
  if (!parsed.success) return Response.json({error: "validation"}, {status: 422});
  const body = Body.safeParse(await request.json());
  if (!body.success) return Response.json({error: "validation"}, {status: 422});
  return Response.json({id: params.id, limit: parsed.data.limit, visible: body.data.visible}, {status: 201});
}
