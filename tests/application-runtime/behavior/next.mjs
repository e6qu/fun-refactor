import { middleware } from "./build/middleware.js";
import { GET } from "./build/guarded/route.js";
import { __order } from "./build/fr-middleware.js";

const passed = await middleware(new Request("http://local/guarded"));
const denied = await GET(new Request("http://local/guarded"));
const allowed = await GET(new Request("http://local/guarded?token=present"));
console.log(JSON.stringify({
  order: __order,
  middlewareStatus: passed.status,
  denied: denied.status,
  allowed: allowed.status,
  allowedBody: await allowed.json(),
}));
