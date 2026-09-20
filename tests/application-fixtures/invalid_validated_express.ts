const Query = z.object({limit: z.number().int()});
function show(req: Request, res: Response) {
  const parsed = Query.safeParse(req.query);
  return res.status(200).json({limit: parsed.data.limit});
}
app.get("/records", show);
