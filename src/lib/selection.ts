/** Pure selection helpers for the page grid (indices are 0-based positions in the page list). */

export interface Selection {
  selected: number[]; // sorted ascending, unique
  anchor: number | null; // where shift-ranges start
}

export const empty: Selection = { selected: [], anchor: null };

const uniqSorted = (a: number[]) => [...new Set(a)].sort((x, y) => x - y);

export function click(sel: Selection, index: number, mods: { shift: boolean; ctrl: boolean }, count: number): Selection {
  if (index < 0 || index >= count) return sel;
  if (mods.shift && sel.anchor !== null) {
    const [a, b] = [Math.min(sel.anchor, index), Math.max(sel.anchor, index)];
    const range = Array.from({ length: b - a + 1 }, (_, i) => a + i);
    return { selected: uniqSorted(mods.ctrl ? [...sel.selected, ...range] : range), anchor: sel.anchor };
  }
  if (mods.ctrl) {
    const has = sel.selected.includes(index);
    return { selected: has ? sel.selected.filter((i) => i !== index) : uniqSorted([...sel.selected, index]), anchor: index };
  }
  return { selected: [index], anchor: index };
}

export function selectAll(count: number): Selection {
  return { selected: Array.from({ length: count }, (_, i) => i), anchor: count ? 0 : null };
}

/** Keep the selection valid after the list length changed. */
export function clamp(sel: Selection, count: number): Selection {
  const selected = sel.selected.filter((i) => i < count);
  return { selected, anchor: sel.anchor !== null && sel.anchor < count ? sel.anchor : selected[0] ?? null };
}

/**
 * Drag-and-drop: the pointer is over the gap before position `dropBefore` (0..count) of the
 * *current* list. Returns the `to` index the backend expects (position in the list after the
 * dragged pages are removed), or null if the move would not change anything.
 */
export function moveTarget(selected: number[], dropBefore: number): number | null {
  const sel = uniqSorted(selected);
  if (sel.length === 0) return null;
  const to = dropBefore - sel.filter((i) => i < dropBefore).length;
  // No-op when the selection is one contiguous block that already starts at `to`.
  const contiguous = sel.every((v, i) => i === 0 || v === sel[i - 1] + 1);
  if (contiguous && sel[0] === to) return null;
  return to;
}

/** `to` for "move selection one step earlier/later" (keyboard reordering). */
export function nudgeTarget(selected: number[], dir: -1 | 1, count: number): number | null {
  const sel = uniqSorted(selected);
  if (sel.length === 0) return null;
  if (dir < 0) return sel[0] === 0 ? null : sel[0] - 1;
  const last = sel[sel.length - 1];
  if (last >= count - 1) return null;
  // after removing the selection, the gap just past the page that follows `last`
  const dropBefore = last + 2;
  return dropBefore - sel.length;
}

export interface BoxLike {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * Pointer-based drag and drop (HTML5 DnD is unavailable when the OS file-drop handler is on):
 * which gap (0..count) is the pointer closest to, given the cell rectangles in list order?
 */
export function dropGap(rects: BoxLike[], x: number, y: number): number {
  if (rects.length === 0) return 0;
  // rows = cells sharing (approximately) the same top
  const rows: { start: number; end: number; top: number; bottom: number }[] = [];
  rects.forEach((r, i) => {
    const row = rows[rows.length - 1];
    if (row && Math.abs(r.top - row.top) < (r.bottom - r.top) / 2) {
      row.end = i;
      row.bottom = Math.max(row.bottom, r.bottom);
    } else {
      rows.push({ start: i, end: i, top: r.top, bottom: r.bottom });
    }
  });
  let best = rows[0];
  let bestDist = Infinity;
  for (const row of rows) {
    const d = y < row.top ? row.top - y : y > row.bottom ? y - row.bottom : 0;
    if (d < bestDist) {
      best = row;
      bestDist = d;
    }
  }
  for (let i = best.start; i <= best.end; i++) {
    const r = rects[i];
    if (x < (r.left + r.right) / 2) return i;
  }
  return best.end + 1;
}
