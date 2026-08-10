/**
 * The window: panes that collapse, gutters that drag, and a layout that survives a
 * reload.
 *
 * A three-column editor is only usable if you can decide how much of it each thing
 * gets — a wide diff wants the result panel, a deep parse tree wants the sidebar. So
 * every boundary is draggable and every sidebar section folds to its title bar, the
 * way an editor does it. The sizes are kept in `localStorage` because a layout you
 * have to set again on every visit is a layout you stop setting.
 *
 * No library: two pointer handlers and a flex-grow each. The alternative is a
 * splitter dependency an order of magnitude larger than the code it replaces.
 */

const STORE = "fun-refactor.layout";

type Layout = {
  /** Pixel widths of the side columns. */
  columns: Record<string, number>;
  /** Flex-grow of each stacked pane. */
  rows: Record<string, number>;
  /** Which panes are folded. */
  collapsed: string[];
};

function read(): Layout {
  try {
    const raw = localStorage.getItem(STORE);
    if (raw) return { columns: {}, rows: {}, collapsed: [], ...JSON.parse(raw) };
  } catch {
    // A private window with storage disabled is not a reason to fail to render.
  }
  return { columns: {}, rows: {}, collapsed: [] };
}

let layout = read();

function save() {
  try {
    localStorage.setItem(STORE, JSON.stringify(layout));
  } catch {
    // Same again: the layout is a convenience, not state anything depends on.
  }
}

/** Ask the editor to remeasure — it does not watch its own container. */
let onResize: () => void = () => {};

export function setResizeHandler(handler: () => void) {
  onResize = handler;
}

// -------------------------------------------------------------------- collapse

function applyCollapsed(id: string, collapsed: boolean) {
  const pane = document.getElementById(id);
  if (!pane) return;
  pane.classList.toggle("collapsed", collapsed);
  const twisty = pane.querySelector(".twisty");
  if (twisty) twisty.textContent = collapsed ? "▸" : "▾";
  // A folded pane must not keep its share of the column, and must come back to the
  // share it had and not to a default.
  if (collapsed) {
    pane.dataset.grow = pane.style.flexGrow || "1";
    pane.style.flexGrow = "0";
  } else {
    pane.style.flexGrow = pane.dataset.grow || "1";
  }
}

function toggle(id: string) {
  const collapsed = !layout.collapsed.includes(id);
  layout.collapsed = collapsed
    ? [...layout.collapsed, id]
    : layout.collapsed.filter((x) => x !== id);
  applyCollapsed(id, collapsed);
  save();
  onResize();
}

// ---------------------------------------------------------------------- drag

/**
 * Start a drag.
 *
 * The move and release listeners go on the window, not on the gutter. A gutter is one
 * pixel wide: a pointer moving at any speed leaves it immediately, and a drag that
 * only tracks while the pointer is over its own handle stops the moment it becomes
 * useful. Pointer capture would fix that too, but it throws when the pointer is
 * already gone, and a throw inside `pointerdown` loses the whole gesture with nothing
 * said — so it is an optimisation here instead of the mechanism.
 */
function onDrag(
  gutter: HTMLElement,
  move: (at: PointerEvent) => void,
  finish: () => void,
) {
  gutter.addEventListener("pointerdown", (down) => {
    down.preventDefault();
    try {
      gutter.setPointerCapture(down.pointerId);
    } catch {
      // No capture: the window listeners below are what actually track the drag.
    }
    gutter.classList.add("dragging");
    // A drag over the editor otherwise selects text in it.
    document.body.classList.add("dragging");

    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      gutter.classList.remove("dragging");
      document.body.classList.remove("dragging");
      finish();
      save();
      onResize();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  });
}

/** Drag a vertical gutter: the column beside it keeps a pixel width. */
function dragColumn(gutter: HTMLElement, columnId: string) {
  const column = document.getElementById(columnId);
  if (!column) return;
  // Which side of the gutter the column is on decides the sign of the drag.
  const leftward =
    (gutter.compareDocumentPosition(column) & Node.DOCUMENT_POSITION_PRECEDING) !== 0;
  let startX = 0;
  let startWidth = 0;

  gutter.addEventListener("pointerdown", (down) => {
    startX = down.clientX;
    startWidth = column.getBoundingClientRect().width;
  });

  onDrag(
    gutter,
    (at) => {
      const delta = leftward ? at.clientX - startX : startX - at.clientX;
      // A column narrower than its own headings is a column you cannot grab again.
      const width = Math.min(Math.max(startWidth + delta, 160), window.innerWidth - 360);
      column.style.width = `${width}px`;
      column.style.flex = `0 0 ${width}px`;
      onResize();
    },
    () => {
      layout.columns[columnId] = column.getBoundingClientRect().width;
    },
  );
}

/** Drag a horizontal gutter: the pane above it grows, the one below shrinks. */
function dragRow(gutter: HTMLElement, aboveId: string) {
  const above = document.getElementById(aboveId);
  if (!above) return;
  // The next pane, skipping the gutter itself and any header between them.
  let below: HTMLElement | null = gutter.nextElementSibling as HTMLElement | null;
  while (below && !below.classList.contains("pane") && below.id !== "action-list") {
    below = below.nextElementSibling as HTMLElement | null;
  }

  /**
   * The vertical padding of a flex item.
   *
   * `flex-basis: 0` sizes the *content* box, so an item's padding is added on top of
   * whatever share of the free space its grow earns. Converting pixels to grow without
   * subtracting it lands the boundary short by exactly the two paddings — nine pixels,
   * every time, in the one pane that has any.
   */
  const padding = (element: HTMLElement | null) => {
    if (!element) return 0;
    const style = getComputedStyle(element);
    return parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
  };

  let startY = 0;
  let startAbove = 0;
  // The two panes either side of this gutter share a fixed amount of space and a
  // fixed amount of flex-grow between them. Dragging moves the boundary within that
  // pair; every other pane in the column is untouched, which is what makes the drag
  // land where the pointer is and not somewhere proportionally near it.
  let pairHeight = 1;
  let pairGrow = 2;
  let padAbove = 0;
  let padPair = 0;

  gutter.addEventListener("pointerdown", (down) => {
    startY = down.clientY;
    startAbove = above.getBoundingClientRect().height;
    const belowHeight = below?.getBoundingClientRect().height ?? 0;
    padAbove = padding(above);
    padPair = padAbove + padding(below);
    pairHeight = Math.max(startAbove + belowHeight - padPair, 1);
    const growOf = (element: HTMLElement | null) =>
      element ? Number(getComputedStyle(element).flexGrow) || 1 : 0;
    pairGrow = growOf(above) + growOf(below);
  });

  onDrag(
    gutter,
    (at) => {
      const wanted = Math.min(
        Math.max(startAbove + (at.clientY - startY), 40),
        pairHeight + padPair - 40,
      );
      // Pixels are what the pointer means; flex-grow is what is stored, so the split
      // holds its proportions when the window itself is resized afterwards.
      const grow = ((wanted - padAbove) / pairHeight) * pairGrow;
      above.style.flexGrow = String(grow);
      if (below) below.style.flexGrow = String(Math.max(pairGrow - grow, 0.05));
      onResize();
    },
    () => {
      layout.rows[aboveId] = Number(above.style.flexGrow) || 1;
      if (below?.id) layout.rows[below.id] = Number(below.style.flexGrow) || 1;
    },
  );
}

// ---------------------------------------------------------------------- start

export function installShell() {
  for (const [id, width] of Object.entries(layout.columns)) {
    const column = document.getElementById(id);
    if (column) {
      column.style.width = `${width}px`;
      column.style.flex = `0 0 ${width}px`;
    }
  }
  for (const [id, grow] of Object.entries(layout.rows)) {
    const pane = document.getElementById(id);
    if (pane) pane.style.flexGrow = String(grow);
  }

  for (const head of document.querySelectorAll<HTMLElement>("[data-collapses]")) {
    const id = head.dataset.collapses!;
    head.addEventListener("click", () => toggle(id));
  }
  for (const id of layout.collapsed) applyCollapsed(id, true);

  for (const gutter of document.querySelectorAll<HTMLElement>(".gutter")) {
    if (gutter.dataset.resizesColumn) dragColumn(gutter, gutter.dataset.resizesColumn);
    else if (gutter.dataset.resizes) dragRow(gutter, gutter.dataset.resizes);
  }

  addEventListener("resize", onResize);
}
