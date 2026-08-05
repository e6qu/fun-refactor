import { CONTRACT, CONTRACT_NOTES, ENDPOINTS } from "./contract-data.js";
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
