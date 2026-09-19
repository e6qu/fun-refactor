export async function load_session(req: { url: string; query?: Record<string, unknown> }): Promise<void> {
  const url = new URL(req.url, "http://local");
  const token = req.query ? req.query["token"] : url.searchParams.get("token");
  if (token !== "present") throw new Error("denied");
}
