/**
 * Turning an answer into something worth reading.
 *
 * The wasm returns the same JSON `fr --json` prints, which means the shapes vary: a
 * list of locations, a rendered tree, a set of diffs, a matrix. Rather than one
 * renderer per action, this reads the shape — a `files` key means a refactoring
 * happened, a `tree` means an analysis printed itself — so an action that returns a
 * familiar shape needs no view code of its own.
 */

export function escapeHtml(text: string): string {
  return String(text).replace(
    /[&<>"]/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] as string,
  );
}

/** A clickable location. */
function goto(file: string, line: number, col = 1, text?: string): string {
  const label = text ?? `${file}:${line}`;
  return `<a data-goto="${escapeHtml(file)}:${line}:${col}">${escapeHtml(label)}</a>`;
}

export function colourDiff(diff: string): string {
  return diff
    .split("\n")
    .map((line) => {
      const cls =
        line.startsWith("+++") || line.startsWith("---") || line.startsWith("@@")
          ? "meta"
          : line.startsWith("+")
            ? "add"
            : line.startsWith("-")
              ? "del"
              : "";
      return `<span class="${cls}">${escapeHtml(line)}</span>`;
    })
    .join("\n");
}

function list(items: string[]): string {
  return `<ul class="plain">${items.join("")}</ul>`;
}

function count(n: number, one: string, many = `${one}s`): string {
  return `<p class="count">${n} ${n === 1 ? one : many}</p>`;
}

/** A refactoring's result: what changed, and what it deliberately did not touch. */
function renderApplied(value: any): string {
  const files: any[] = value.files ?? [];
  const notes: string[] = value.notes ?? [];
  const warnings: any[] = value.warnings ?? [];

  const head = files.length
    ? count(files.length, "file changed", "files changed")
    : `<p class="hint">Nothing to change: the analysis found no site it was certain about.</p>`;

  const diffs = files
    .map(
      (f) =>
        `<details open><summary>${escapeHtml(f.path)}</summary>` +
        `<pre class="diff">${colourDiff(f.diff)}</pre></details>`,
    )
    .join("");

  const left = warnings.length
    ? `<p class="warn">Left alone — check these yourself:</p>` +
      list(
        warnings.map(
          (w) =>
            `<li>${escapeHtml(w.file ?? "")} <span class="dim">${escapeHtml(
              w.detail ?? w.kind ?? "",
            )}</span></li>`,
        ),
      )
    : "";

  const said = notes.length
    ? `<p class="warn">Notes:</p>` + list(notes.map((n) => `<li>${escapeHtml(n)}</li>`))
    : "";

  return head + diffs + left + said;
}

function renderStats(s: any): string {
  return (
    `<dl class="stats">
       <dt>files</dt><dd>${s.files}</dd>
       <dt>symbols</dt><dd>${s.symbols}</dd>
       <dt>references</dt><dd>${s.references}</dd>
     </dl>` +
    list(
      s.languages.map(
        ([name, n]: [string, number]) =>
          `<li><span class="chip">${escapeHtml(name)}</span> <span class="dim">${n} file${
            n === 1 ? "" : "s"
          }</span></li>`,
      ),
    ) +
    (s.unparsed?.length
      ? `<p class="warn">${s.unparsed.length} file(s) did not parse, so references in them are not counted.</p>`
      : "") +
    (s.unsupported?.length
      ? `<p class="warn">${s.unsupported.length} file(s) left out: this build has no grammar for them.</p>`
      : "")
  );
}

/**
 * The capability matrix.
 *
 * Each language arrives as a `[name, support]` pair — the shape `Vec<(&str, Support)>`
 * serialises to — and `support` is `{support: "yes"}` or a tagged variant carrying
 * `because`. That reason is the interesting half: "not applicable" and "refused" are
 * different claims, and the sentence explaining which is why the matrix is generated
 * from the code rather than written by hand. An earlier version of this reader
 * expected objects with a `language` key and rendered fifteen chips reading
 * "undefined" per row.
 */
function renderCapabilities(matrix: any): string {
  const rows: any[] = Array.isArray(matrix) ? matrix : (matrix.rows ?? []);
  if (!rows.length) return `<pre class="raw">${escapeHtml(JSON.stringify(matrix, null, 2))}</pre>`;

  const chipFor = (entry: any): string => {
    // `["rust", {support: "yes"}]`, or a bare string in the simplest shape.
    const [language, support] = Array.isArray(entry) ? entry : [entry, { support: "yes" }];
    const kind = typeof support === "string" ? support : (support?.support ?? "yes");
    const because = typeof support === "object" ? support?.because : undefined;
    const state = kind === "yes" ? "yes" : "no";
    const title = because
      ? `${kind.replace(/-/g, " ")}: ${because}`
      : kind.replace(/-/g, " ");
    return (
      `<span class="chip ${state}" title="${escapeHtml(title)}">${escapeHtml(String(language))}` +
      (kind === "yes" ? "" : ` <span class="dim">${kind === "not-applicable" ? "n/a" : "—"}</span>`) +
      `</span>`
    );
  };

  return rows
    .map((row: any) => {
      const languages: any[] = row.languages ?? row.supported ?? [];
      const supported = languages.filter((l) => {
        const support = Array.isArray(l) ? l[1] : null;
        return (typeof support === "string" ? support : support?.support) === "yes";
      }).length;
      return (
        `<p class="count">${escapeHtml(row.capability ?? row.name ?? "")} ` +
        `<span class="dim">${supported}/${languages.length}` +
        (row.command ? ` · ${escapeHtml(row.command)}` : "") +
        `</span></p>` +
        `<p class="chips">${languages.map(chipFor).join("")}</p>`
      );
    })
    .join("");
}

/** The same words the terminal prints for a definition's role. */
const ROLE: Record<string, string> = {
  primary: "definition",
  "same-entity": "also declared here",
  implementation: "implementation",
};

function renderDefinitions(value: any): string {
  const found: any[] = value.definitions ?? [];
  if (!found.length) return `<p class="hint">Nothing is defined at that position.</p>`;
  return (
    count(found.length, "definition") +
    list(
      found.map(
        (d) =>
          `<li>${goto(d.location.file, d.location.line, d.location.col)}` +
          ` <span class="chip">${escapeHtml(d.kind ?? "")}</span>` +
          ` <span class="dim">${escapeHtml(ROLE[d.role] ?? d.role ?? "")}</span></li>`,
      ),
    )
  );
}

function renderUsages(value: any): string {
  const here: any[] = value.usages ?? [];
  const elsewhere: any[] = value.same_name_elsewhere ?? [];
  const one = (u: any) =>
    `<li>${goto(u.location.file, u.location.line, u.location.col)}` +
    ` <span class="tier ${escapeHtml(u.confidence)}">${escapeHtml(u.confidence)}</span>` +
    (u.within ? ` <span class="dim">in ${escapeHtml(u.within)}</span>` : "") +
    `</li>`;
  return (
    count(here.length, "use") +
    list(here.map(one)) +
    (elsewhere.length
      ? `<p class="warn">${elsewhere.length} occurrence(s) share the name but resolved
          elsewhere or not at all. They are not uses of this symbol.</p>` +
        list(elsewhere.map(one))
      : "")
  );
}

function renderReferences(refs: any[]): string {
  if (!refs.length) return `<p class="hint">No references resolve to that symbol.</p>`;
  return (
    count(refs.length, "reference") +
    list(
      refs.map(
        (r) =>
          `<li>${goto(r.path, r.line, r.col)}` +
          ` <span class="tier ${escapeHtml(r.confidence)}">${escapeHtml(r.confidence)}</span></li>`,
      ),
    )
  );
}

function renderUnused(dead: any[]): string {
  if (!dead.length) return `<p class="hint">Everything here is reachable.</p>`;
  const internal = dead.filter((d) => !d.exported);
  return (
    `<p class="count">${dead.length} with no detected use, ${internal.length} of them unexported</p>` +
    list(
      dead
        .slice(0, 120)
        .map(
          (d) =>
            `<li><span class="chip">${escapeHtml(d.kind)}</span> <strong>${escapeHtml(
              d.name,
            )}</strong> <span class="dim">${escapeHtml(d.path)}</span>` +
            (d.exported ? ` <span class="tier name-only">exported</span>` : "") +
            `</li>`,
        ),
    ) +
    `<p class="hint">An exported symbol nothing here uses may be a public API rather than
      dead code — which it is, is not the tool's call.</p>`
  );
}

function renderDuplicates(classes: any[]): string {
  if (!classes.length) return `<p class="hint">No duplication at this threshold.</p>`;
  return classes
    .slice(0, 25)
    .map(
      (c) =>
        `<p class="count">${c.instances.length} copies, ${c.tokens} tokens each</p>` +
        list(
          c.instances.map(
            (i: any) =>
              `<li>${goto(i.file, i.start_line, 1, `${i.file}:${i.start_line}-${i.end_line}`)}</li>`,
          ),
        ),
    )
    .join("");
}

function renderEntrypoints(found: any[]): string {
  if (!found.length) {
    return `<p class="warn">No entry points at all. Everything unexported will read as dead.</p>`;
  }
  const byKind = new Map<string, any[]>();
  for (const e of found) {
    if (!byKind.has(e.kind)) byKind.set(e.kind, []);
    byKind.get(e.kind)!.push(e);
  }
  return [...byKind.entries()]
    .map(
      ([kind, entries]) =>
        `<p class="count">${escapeHtml(kind)} <span class="dim">${entries.length}</span></p>` +
        list(
          entries.map(
            (e: any) =>
              `<li>${goto(e.path, e.line, 1, e.name)} <span class="dim">${escapeHtml(
                e.path,
              )}</span></li>`,
          ),
        ),
    )
    .join("");
}

/** A file outline. The symbols carry a position but not a path: it is the open file. */
function renderSymbols(symbols: any[], path: string): string {
  if (!symbols.length) return `<p class="hint">Nothing is defined in this file.</p>`;
  return (
    count(symbols.length, "definition") +
    list(
      symbols.map(
        (s) =>
          `<li><span class="chip">${escapeHtml(s.kind)}</span> ` +
          goto(path, s.line, s.col, s.name) +
          (s.exported ? ` <span class="tier name-only">exported</span>` : "") +
          `</li>`,
      ),
    )
  );
}

/**
 * Render whatever came back.
 *
 * `path` is the open file, used where an answer gives a position without repeating
 * which file it is in. `empty` is what the caller wants said when the answer is an
 * empty list — the one shape that cannot say anything about itself.
 */
export function render(value: any, path: string, empty?: string): string {
  if (value === null || value === undefined) return "";

  if (Array.isArray(value)) {
    if (!value.length) {
      return `<p class="hint">${escapeHtml(empty ?? "Nothing to report.")}</p>`;
    }
    const first = value[0];
    if ("confidence" in first && "path" in first) return renderReferences(value);
    if ("exported" in first && "kind" in first && "path" in first) return renderUnused(value);
    if ("instances" in first) return renderDuplicates(value);
    if ("kind" in first && "line" in first && "path" in first) return renderEntrypoints(value);
    if ("kind" in first && "line" in first && "col" in first) return renderSymbols(value, path);
    if ("capability" in first || "languages" in first) return renderCapabilities(value);
    if ("name" in first && "describes" in first) {
      return (
        count(value.length, "transformation applies", "transformations apply") +
        list(
          value.map(
            (r: any) =>
              `<li><strong>${escapeHtml(r.name)}</strong> <span class="dim">${escapeHtml(
                r.describes,
              )}</span></li>`,
          ),
        )
      );
    }
    return `<pre class="raw">${escapeHtml(JSON.stringify(value, null, 2))}</pre>`;
  }

  if ("files" in value && Array.isArray(value.files) && "warnings" in value) {
    return renderApplied(value);
  }
  if ("tree" in value) {
    return `<pre class="tree">${escapeHtml(value.tree)}</pre>`;
  }
  if ("definitions" in value) return renderDefinitions(value);
  if ("usages" in value) return renderUsages(value);
  if ("symbols" in value && "references" in value && "languages" in value) {
    return renderStats(value);
  }
  if ("functions" in value && "edges" in value) {
    return `<dl class="stats">
      <dt>functions</dt><dd>${value.functions}</dd>
      <dt>call edges</dt><dd>${value.edges}</dd>
      <dt>dispatch edges</dt><dd>${value.hierarchy_edges}</dd>
    </dl>
    <p class="hint">A dispatch edge is a candidate, not a proven call: which
      implementation runs is a runtime fact.</p>`;
  }
  if ("rows" in value || "capability" in value) return renderCapabilities(value);

  return `<pre class="raw">${escapeHtml(JSON.stringify(value, null, 2))}</pre>`;
}
