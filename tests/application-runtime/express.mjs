import express from "express";
import { readFile } from "node:fs/promises";
import router from "./build/routes.js";

const app = express();
app.use(router);
const server = app.listen(0, "127.0.0.1");
try {
  await new Promise((resolve, reject) => {
    server.once("listening", resolve);
    server.once("error", reject);
  });
  const cases = JSON.parse(await readFile("cases.json", "utf8"));
  const results = [];
  for (const test of cases) {
    const response = await fetch(`http://127.0.0.1:${server.address().port}${test.url}`, { method: test.method });
    results.push({ status: response.status, body: await response.json() });
  }
  console.log(JSON.stringify(results));
} finally {
  await new Promise((resolve) => server.close(resolve));
}
