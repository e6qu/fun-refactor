// Before / After / Diff, and the tool's own output beside them.
//
// Shared by catalog.html and translate.html because both answer the same question in
// the same shape: here is a file, here is one command, here is what the command did
// and what it said about it.
//
// Everything rendered here comes from the generated data files, which are produced by
// running the real binary (`cargo test --test site_data`). Nothing on either page is
// written by hand, so nothing on either page can drift from what the tool does.

const escape = (text) =>
  text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/** Colour a unified diff the way the terminal does. */
function paintDiff(text) {
  return text
    .split("\n")
    .map((line) => {
      if (line.startsWith("+++") || line.startsWith("---")) {
        return `<span class="dim">${escape(line)}</span>`;
      }
      if (line.startsWith("@@")) return `<span class="dim">${escape(line)}</span>`;
      if (line.startsWith("+")) return `<span class="add">${escape(line)}</span>`;
      if (line.startsWith("-")) return `<span class="del">${escape(line)}</span>`;
      return escape(line);
    })
    .join("\n");
}

/**
 * Split what the command printed into its diff and everything else.
 *
 * By position, not by pattern. Matching diff lines with a regular expression means
 * treating a leading space as "context line", and the report's own lines are indented
 * — so `  signatures: 3 complete` was filtered out as though it were part of the diff,
 * and three quarters of what the tool said never reached the page.
 */
function split(output) {
  const lines = output.split("\n");
  const start = lines.findIndex((line) => line.startsWith("--- a/"));
  if (start === -1) return { diff: "", report: output.trim() };

  // The hunk runs to the first blank line that is not followed by more of it.
  let end = lines.length;
  for (let i = start + 1; i < lines.length; i++) {
    if (lines[i] === "" && !/^[-+ @]/.test(lines[i + 1] ?? "")) {
      end = i;
      break;
    }
  }
  return {
    diff: lines.slice(start, end).join("\n").trimEnd(),
    report: [...lines.slice(0, start), ...lines.slice(end)].join("\n").trim(),
  };
}

/** Colour the one line that matters in a report. */
function paintReport(text) {
  return text
    .split("\n")
    .map((line) =>
      line.startsWith("Error:")
        ? `<span class="refuse">${escape(line)}</span>`
        : escape(line),
    )
    .join("\n");
}

/**
 * Render one specimen into `host`.
 *
 * `panes` is `[{label, body, paint}]`; the first is shown and the rest are a click
 * away. Deliberately the whole of the interactivity: the page is a page, and a reader
 * who wants to run the thing has the command sitting above it.
 */
export function specimen(host, { title, command, panes, caption }) {
  const id = Math.random().toString(36).slice(2, 8);
  host.innerHTML = `
    <div class="term">
      <div class="term-bar">
        <i class="dot"></i><i class="dot"></i><i class="dot"></i>
        <span class="term-title">${escape(title)}</span>
        <span class="pane-tabs" role="tablist">
          ${panes
            .map(
              (pane, i) =>
                `<button role="tab" data-pane="${i}" aria-selected="${i === 0}"
                         aria-controls="p-${id}">${escape(pane.label)}</button>`,
            )
            .join("")}
        </span>
      </div>
      <div class="term-body"><pre id="p-${id}"></pre></div>
    </div>
    ${command ? `<div class="specimen-cmd"><code>${escape(command)}</code><button class="copy" title="Copy">copy</button></div>` : ""}
    ${caption ? `<p class="demo-caption">${caption}</p>` : ""}
  `;

  const screen = host.querySelector(`#p-${id}`);
  const tabs = [...host.querySelectorAll("[role=tab]")];
  const show = (index) => {
    const pane = panes[index];
    screen.innerHTML = pane.paint ? pane.paint(pane.body) : escape(pane.body);
    tabs.forEach((tab, i) => tab.setAttribute("aria-selected", String(i === index)));
  };
  tabs.forEach((tab, i) => tab.addEventListener("click", () => show(i)));
  show(0);

  const copy = host.querySelector(".copy");
  if (copy) {
    copy.addEventListener("click", async () => {
      await navigator.clipboard.writeText(command);
      copy.textContent = "copied";
      setTimeout(() => (copy.textContent = "copy"), 1200);
    });
  }
}

/** The three panes a before-and-after always has. */
export function beforeAfterDiff(before, after, output) {
  const { diff, report } = split(output);
  const panes = [{ label: "Before", body: before }];
  if (after) panes.push({ label: "After", body: after });
  if (diff) panes.push({ label: "Diff", body: diff, paint: paintDiff });
  if (report) panes.push({ label: "What it said", body: report, paint: paintReport });
  return panes;
}

export { escape, paintDiff, paintReport, split };
