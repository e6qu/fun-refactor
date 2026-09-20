const Query = z.object({limit: z.number().int()});
export async function GET(request: Request) {
  const parsed = Query.safeParse(Object.fromEntries(new URL(request.url).searchParams));
  if (!parsed.success) return Response.json({error: "validation"}, {status: 422});
  return Response.json({limit: parsed.data.limit}, {status: 200});
}
