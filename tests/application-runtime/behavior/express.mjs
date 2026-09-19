import express from "express";
import router from "./build/routes.js";

const app = express();
app.use(router);
const server = app.listen(0, "127.0.0.1");
try {
  await new Promise((resolve, reject) => {
    server.once("listening", resolve);
    server.once("error", reject);
  });
  const results = [];
  for (const url of ["/guarded", "/guarded?token=present"]) {
    const response = await fetch(`http://127.0.0.1:${server.address().port}${url}`);
    results.push({
      status: response.status,
      body: await response.json(),
      order: response.headers.get("x-fr-order"),
    });
  }
  console.log(JSON.stringify(results));
} finally {
  await new Promise((resolve) => server.close(resolve));
}
