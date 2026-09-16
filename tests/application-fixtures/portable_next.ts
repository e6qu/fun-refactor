export async function GET(_request: Request, context: {params: Promise<{id: string}>}) {
  const params = await context.params;
  return Response.json({id: params["id"], framework: "portable"}, {status: 201});
}
