/**
 * Which playground you get.
 *
 * `?render_as=` picks the rendering:
 *
 *   html       the page (default)
 *   json       the same view, described rather than drawn — machine-readable
 *   json_data  the analysis underneath it, with no view at all
 *
 * The two JSON modes exist so something other than a person can ask this page what it
 * knows: a script, a test, an agent. They never import the app, so Monaco — four
 * megabytes of editor — is not fetched to answer a question about a repository.
 *
 * An unrecognised value is refused rather than quietly treated as `html`: a typo in
 * `render_as` that silently returns a web page is the kind of thing a caller notices
 * three layers downstream.
 */

const MODES = ["html", "json", "json_data"] as const;
export type Mode = (typeof MODES)[number];

const asked = new URLSearchParams(location.search).get("render_as");

function refuse(message: string) {
  document.title = "fun-refactor — bad request";
  document.body.textContent = JSON.stringify(
    { error: message, render_as: MODES },
    null,
    2,
  );
  document.body.style.cssText =
    "font:13px ui-monospace,monospace;white-space:pre-wrap;padding:1rem";
}

if (asked !== null && !MODES.includes(asked as Mode)) {
  refuse(`'${asked}' is not a rendering. Use one of: ${MODES.join(", ")}.`);
} else {
  const mode = (asked ?? "html") as Mode;
  if (mode === "html") {
    import("./app");
  } else {
    import("./serialise").then((module) => module.emit(mode));
  }
}
