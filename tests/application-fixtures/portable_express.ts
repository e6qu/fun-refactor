function showExpress(req: Request, res: Response) {
  return res.status(201).json({id: req.params["id"], framework: "portable"});
}

app.get("/express/:id", showExpress);
