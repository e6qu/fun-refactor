/**
 * Loading a public repository in the browser, with no server of our own.
 *
 * Two requests answer "what is in here": the git tree API returns every path in one
 * call, and each file's bytes come from raw.githubusercontent.com, which serves
 * permissive CORS headers and is not counted against the API rate limit. The tree
 * call is, which is why there is exactly one of them.
 *
 * A repository is fetched into memory, so it has to be bounded. The caps below are
 * about what a browser tab can hold and what a person will wait for, not about what
 * the analysis can handle — the same code indexes 16,000 files in ten seconds
 * natively. Anything dropped is reported rather than silently skipped: a file list
 * that quietly stops at 400 makes every later answer wrong in a way nobody can see.
 */

export interface LoadOptions {
  owner: string;
  repo: string;
  /** A branch, tag or commit. Empty means the repository's default branch. */
  ref?: string;
  /** Only fetch under this path, which is how a large repository stays loadable. */
  prefix?: string;
  maxFiles?: number;
  maxBytes?: number;
  onProgress?: (done: number, total: number, note: string) => void;
}

export interface LoadResult {
  files: Record<string, string>;
  ref: string;
  /** What was left out, and why, in the caller's words. */
  skipped: { path: string; reason: string }[];
  truncatedTree: boolean;
}

/** Extensions the tool has a grammar for. Anything else is weight without answers. */
const SUPPORTED = new Set([
  "rs", "go", "ts", "tsx", "js", "jsx", "py", "zig", "sh", "bash",
  "html", "htm", "css", "scss", "tf", "tfvars", "yaml", "yml", "xml", "md",
]);

/** Files with no extension that still matter. */
const SUPPORTED_NAMES = new Set(["Chart.yaml", "chart.yaml", "values.yaml"]);

function isSupported(path: string): boolean {
  const name = path.split("/").pop() ?? "";
  if (SUPPORTED_NAMES.has(name)) return true;
  const dot = name.lastIndexOf(".");
  return dot > 0 && SUPPORTED.has(name.slice(dot + 1).toLowerCase());
}

async function json(url: string): Promise<any> {
  const response = await fetch(url, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (response.status === 403 || response.status === 429) {
    throw new Error(
      "GitHub rate-limited this browser (60 requests an hour for anonymous callers). " +
        "Wait a few minutes, or pick a repository you have already loaded.",
    );
  }
  if (!response.ok) {
    throw new Error(`GitHub returned ${response.status} for ${url}`);
  }
  return response.json();
}

export async function loadRepository(options: LoadOptions): Promise<LoadResult> {
  const {
    owner,
    repo,
    prefix = "",
    maxFiles = 400,
    maxBytes = 6 * 1024 * 1024,
    onProgress = () => {},
  } = options;

  let ref = options.ref?.trim() ?? "";
  if (!ref) {
    onProgress(0, 1, "finding the default branch");
    const meta = await json(`https://api.github.com/repos/${owner}/${repo}`);
    ref = meta.default_branch;
  }

  onProgress(0, 1, `listing ${owner}/${repo}@${ref}`);
  const tree = await json(
    `https://api.github.com/repos/${owner}/${repo}/git/trees/${encodeURIComponent(ref)}?recursive=1`,
  );

  const skipped: { path: string; reason: string }[] = [];
  const wanted: { path: string; size: number }[] = [];

  for (const entry of tree.tree ?? []) {
    if (entry.type !== "blob") continue;
    if (prefix && !entry.path.startsWith(prefix)) continue;
    if (!isSupported(entry.path)) continue;
    wanted.push({ path: entry.path, size: entry.size ?? 0 });
  }

  // Smallest first: a repository's value here is breadth, and one 900 KB generated
  // file would otherwise crowd out two hundred real ones.
  wanted.sort((a, b) => a.size - b.size || a.path.localeCompare(b.path));

  const chosen: string[] = [];
  let bytes = 0;
  for (const entry of wanted) {
    if (chosen.length >= maxFiles) {
      skipped.push({ path: entry.path, reason: `over the ${maxFiles}-file limit` });
      continue;
    }
    if (bytes + entry.size > maxBytes) {
      skipped.push({ path: entry.path, reason: "over the total size limit" });
      continue;
    }
    chosen.push(entry.path);
    bytes += entry.size;
  }

  const files: Record<string, string> = {};
  let done = 0;
  const base = `https://raw.githubusercontent.com/${owner}/${repo}/${encodeURIComponent(ref)}/`;

  // Eight at a time: enough to hide the round trips, few enough that a browser does
  // not queue them anyway.
  const queue = [...chosen];
  async function worker() {
    for (;;) {
      const path = queue.shift();
      if (path === undefined) return;
      try {
        const response = await fetch(base + path.split("/").map(encodeURIComponent).join("/"));
        if (!response.ok) {
          skipped.push({ path, reason: `fetch returned ${response.status}` });
        } else {
          files[path] = await response.text();
        }
      } catch (e) {
        skipped.push({ path, reason: String(e) });
      }
      done += 1;
      onProgress(done, chosen.length, path);
    }
  }
  await Promise.all(Array.from({ length: 8 }, worker));

  return { files, ref, skipped, truncatedTree: Boolean(tree.truncated) };
}

/** `owner/repo`, a full GitHub URL, or either with a `/tree/<ref>/<path>` tail. */
export function parseTarget(input: string): Omit<LoadOptions, "onProgress"> | null {
  const text = input.trim().replace(/^https?:\/\/(www\.)?github\.com\//, "");
  const parts = text.split("/").filter(Boolean);
  if (parts.length < 2) return null;
  const [owner, repo, keyword, ref, ...rest] = parts;
  if (keyword === "tree" || keyword === "blob") {
    return { owner, repo, ref, prefix: rest.join("/") };
  }
  return { owner, repo };
}
