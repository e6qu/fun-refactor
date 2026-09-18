import { readFile } from "node:fs/promises";
import { POST } from "./build/route.js";

const cases = JSON.parse(await readFile("cases.json", "utf8"));
const results = [];
for (const test of cases.filter((item) => item.url.startsWith("/validated/"))) {
  const init = { method: test.method };
  if (test.body !== undefined) {
    init.headers = { "content-type": "application/json" };
    init.body = JSON.stringify(test.body);
  }
  const request = new Request(`http://local${test.url}`, init);
  const response = await POST(request, { params: Promise.resolve({ id: "chosen" }) });
  results.push({ status: response.status, body: await response.json() });
}
console.log(JSON.stringify(results));
