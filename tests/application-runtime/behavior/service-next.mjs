import { createServer } from "node:http";
import { GET as records } from "./build/records/route.js";
import { GET as feed } from "./build/feed/route.js";

const server = createServer(async (req, res) => {
  const url = `http://127.0.0.1:${server.address().port}${req.url}`;
  const handler = req.url === "/records" ? records : req.url === "/feed" ? feed : null;
  if (handler === null) {
    res.writeHead(404);
    res.end("{}");
    return;
  }
  const response = await handler(new Request(url));
  res.writeHead(response.status, { "content-type": "application/json" });
  res.end(await response.text());
});
server.listen(0, "127.0.0.1");
try {
  await new Promise((resolve, reject) => {
    server.once("listening", resolve);
    server.once("error", reject);
  });
  const results = [];
  for (const path of ["/records", "/feed"]) {
    const response = await fetch(`http://127.0.0.1:${server.address().port}${path}`);
    results.push({ status: response.status, body: await response.json() });
  }
  console.log(JSON.stringify(results));
} finally {
  await new Promise((resolve) => server.close(resolve));
}
