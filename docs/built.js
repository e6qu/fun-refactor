// What the published site was built from.
//
// A failed deploy leaves the site silently stale: the tests are green, the page looks
// finished, and it is three commits behind what it claims to show. Nothing said so.
// The workflow overwrites this file with the commit it deployed, so the answer to "is
// this current?" is on the page instead of in a log nobody reads.

export const BUILT = { commit: "working copy", at: "" };

const target = document.getElementById("built");
if (target) {
  const at = BUILT.at ? ` on ${BUILT.at}` : "";
  target.textContent = `Built from ${BUILT.commit}${at}.`;
}
