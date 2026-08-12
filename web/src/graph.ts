// The call graph around the cursor, drawn as SVG.
//
// `graph()` answers with three counts, which says how big the graph is and nothing
// about its shape. This asks `graph_around`, which returns the nodes and the edges
// within a few hops of one symbol, and lays them out in columns by distance.

export type GraphNode = {
  id: number;
  name: string;
  file: string;
  line: number;
  col: number;
  rank: number;
};

export type GraphEdge = { from: number; to: number; kind: "call" | "dispatch" };

export type Drawing = {
  nodes: GraphNode[];
  edges: GraphEdge[];
  root: number;
  more: boolean;
};

const NODE_HEIGHT = 26;
const ROW_GAP = 12;
const COLUMN_GAP = 90;
const CHARACTER = 6.6;
const PADDING = 14;

/** The last segment of a qualified name, which is what a reader scans for. */
function shortName(name: string): string {
  const parts = name.split(/::|\./);
  return parts[parts.length - 1] || name;
}

function width(node: GraphNode): number {
  return Math.max(70, Math.min(220, shortName(node.name).length * CHARACTER + 18));
}

/**
 * Place every node, then draw.
 *
 * Columns are the distance from the symbol the reader asked about: callers to the
 * left, callees to the right. A column holds its nodes in one vertical run, which
 * keeps the picture readable up to a few dozen nodes and needs no physics.
 */
export function draw(
  drawing: Drawing,
  onPick: (node: GraphNode) => void,
): SVGSVGElement | HTMLElement {
  if (drawing.nodes.length <= 1 && drawing.edges.length === 0) {
    const empty = document.createElement("p");
    empty.className = "graph-empty";
    empty.textContent =
      "Nothing calls this and it calls nothing that resolves. A call graph needs a " +
      "function; a value or a type has no edges.";
    return empty;
  }

  const columns = new Map<number, GraphNode[]>();
  for (const node of drawing.nodes) {
    const column = columns.get(node.rank) ?? [];
    column.push(node);
    columns.set(node.rank, column);
  }
  const ranks = [...columns.keys()].sort((a, b) => a - b);

  const place = new Map<number, { x: number; y: number; w: number }>();
  let x = PADDING;
  let widest = 0;
  for (const rank of ranks) {
    const column = columns.get(rank)!;
    const columnWidth = Math.max(...column.map(width));
    let y = PADDING;
    for (const node of column) {
      place.set(node.id, { x, y, w: columnWidth });
      y += NODE_HEIGHT + ROW_GAP;
    }
    widest = Math.max(widest, y);
    x += columnWidth + COLUMN_GAP;
  }

  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("width", String(x));
  svg.setAttribute("height", String(widest + PADDING));

  const marker = document.createElementNS("http://www.w3.org/2000/svg", "marker");
  marker.setAttribute("id", "arrow");
  marker.setAttribute("viewBox", "0 0 10 10");
  marker.setAttribute("refX", "9");
  marker.setAttribute("refY", "5");
  marker.setAttribute("markerWidth", "6");
  marker.setAttribute("markerHeight", "6");
  marker.setAttribute("orient", "auto-start-reverse");
  const head = document.createElementNS("http://www.w3.org/2000/svg", "path");
  head.setAttribute("d", "M 0 0 L 10 5 L 0 10 z");
  head.setAttribute("fill", "currentColor");
  marker.appendChild(head);
  const defs = document.createElementNS("http://www.w3.org/2000/svg", "defs");
  defs.appendChild(marker);
  svg.appendChild(defs);

  for (const edge of drawing.edges) {
    const from = place.get(edge.from);
    const to = place.get(edge.to);
    if (!from || !to) continue;
    const x1 = from.x + from.w;
    const y1 = from.y + NODE_HEIGHT / 2;
    const x2 = to.x;
    const y2 = to.y + NODE_HEIGHT / 2;
    const mid = (x1 + x2) / 2;
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", `M ${x1} ${y1} C ${mid} ${y1}, ${mid} ${y2}, ${x2} ${y2}`);
    path.setAttribute("class", `graph-edge ${edge.kind}`);
    path.setAttribute("marker-end", "url(#arrow)");
    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent =
      edge.kind === "dispatch"
        ? "One implementation this call could reach. The program chooses while it runs."
        : "A resolved call.";
    path.appendChild(title);
    svg.appendChild(path);
  }

  for (const node of drawing.nodes) {
    const at = place.get(node.id)!;
    const group = document.createElementNS("http://www.w3.org/2000/svg", "g");
    group.setAttribute("class", `graph-node${node.id === drawing.root ? " root" : ""}`);
    group.addEventListener("click", () => onPick(node));

    const box = document.createElementNS("http://www.w3.org/2000/svg", "rect");
    box.setAttribute("x", String(at.x));
    box.setAttribute("y", String(at.y));
    box.setAttribute("width", String(at.w));
    box.setAttribute("height", String(NODE_HEIGHT));
    group.appendChild(box);

    const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
    label.setAttribute("x", String(at.x + 9));
    label.setAttribute("y", String(at.y + NODE_HEIGHT / 2 + 4));
    label.textContent = shortName(node.name);
    group.appendChild(label);

    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
    title.textContent = `${node.name}\n${node.file}:${node.line}`;
    group.appendChild(title);

    svg.appendChild(group);
  }
  return svg;
}
