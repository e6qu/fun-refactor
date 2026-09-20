import { z } from "zod";
const Query = z.object({limit: z.coerce.number().int()});
const Body = z.object({visible: z.boolean()});
function show(req: Request, res: Response) {
  const parsed = Query.safeParse(req.query);
  if (!parsed.success) return res.status(422).json({error: "validation"});
  const body = Body.safeParse(req.body);
  if (!body.success) return res.status(422).json({error: "validation"});
  return res.status(201).json({id: req.params.id, limit: parsed.data.limit, visible: body.data.visible});
}
app.post("/records/:id", show);
