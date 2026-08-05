import {
  CONTRACT,
  CONTRACT_NOTES,
  CROSSED,
  ENDPOINTS,
  SURVIVED,
} from "./contract-data.js";
import { specimen, split, paintDiff, paintReport, escape } from "./panes.js";

// The contract, and everything it could not settle. Two panes, because they are two
// halves of one answer: a baseline that quietly invents an entry is worse than no
// baseline, so what it could not determine is beside it rather than filled in.
specimen(document.getElementById("contract"), {
  title: "contract.yaml — derived from the tree, before the rewrite",
  command: "fr openapi --yaml > contract.yaml",
  panes: [
    { label: "The document", body: CONTRACT },
    { label: "What it does not settle", body: CONTRACT_NOTES, paint: paintReport },
  ],
});

const host = document.getElementById("endpoints");

host.innerHTML = ENDPOINTS.map(
  (endpoint) => `
  <article class="entry" id="${escape(endpoint.route.replace(/[^\w]+/g, "-"))}">
    <h3>${escape(endpoint.shape)}</h3>
    <p class="entry-intent"><code>${escape(endpoint.route)}</code></p>
    <p class="entry-invariant"><strong>Unchanged:</strong> the URL, the method and the
    shape of the body. Only the language, the framework and the way the handler reaches
    its arguments moved.</p>
    <div class="specimen" data-route="${escape(endpoint.route)}"></div>
  </article>`,
).join("");

for (const endpoint of ENDPOINTS) {
  const { report } = split(endpoint.report);
  const { diff } = split(endpoint.report);
  const panes = [
    { label: "Next.js", body: endpoint.before },
    { label: "FastAPI", body: endpoint.after },
  ];
  // The diff is all additions: nothing was changed, a file was written beside the one
  // that was read. Shown anyway, because that is the shape of the edit the tool makes.
  if (diff) panes.push({ label: "Diff", body: diff, paint: paintDiff });
  if (report) panes.push({ label: "What it said", body: report, paint: paintReport });
  specimen(host.querySelector(`.specimen[data-route="${CSS.escape(endpoint.route)}"]`), {
    title: endpoint.route,
    command: endpoint.command,
    panes,
    caption: escape(endpoint.note),
  });
}

// Did the contract survive? Read the *other* side — the decorators and the signatures
// the translated router declares — and compare, operation by operation. This is the
// check you can make without running the service, and it catches the failure the whole
// exercise is about: an endpoint that did not survive, or a path that changed shape.
const survived = document.getElementById("survived");
if (survived) {
  const row = (op, cls) => `<li class="${cls}"><code>${escape(op)}</code></li>`;
  const lost = new Set(SURVIVED.lost);
  const gained = new Set(SURVIVED.gained);
  survived.innerHTML = `
    <div class="contract-columns">
      <div>
        <h4>Declared by the Next.js tree</h4>
        <ul class="op-list">${SURVIVED.before.map((o) => row(o, lost.has(o) ? "del" : "")).join("")}</ul>
      </div>
      <div>
        <h4>Declared by the FastAPI router it became</h4>
        <ul class="op-list">${SURVIVED.after.map((o) => row(o, gained.has(o) ? "add" : "")).join("")}</ul>
      </div>
    </div>`;
}

const crossed = document.getElementById("crossed");
if (crossed) {
  specimen(crossed, {
    title: "the same command, on the other side",
    command: "fr openapi --yaml",
    panes: [{ label: "Read back off the FastAPI router", body: CROSSED }],
  });
}
