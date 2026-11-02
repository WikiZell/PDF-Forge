import { describe, expect, it } from "vitest";
import { click, clamp, empty, moveTarget, nudgeTarget, selectAll } from "./selection";

const none = { shift: false, ctrl: false };

describe("selection", () => {
  it("plain click selects one and sets the anchor", () => {
    expect(click(empty, 3, none, 10)).toEqual({ selected: [3], anchor: 3 });
  });
  it("ctrl toggles and keeps others", () => {
    let s = click(empty, 1, none, 10);
    s = click(s, 4, { shift: false, ctrl: true }, 10);
    expect(s.selected).toEqual([1, 4]);
    s = click(s, 1, { shift: false, ctrl: true }, 10);
    expect(s.selected).toEqual([4]);
  });
  it("shift selects a range from the anchor in either direction", () => {
    let s = click(empty, 5, none, 10);
    s = click(s, 2, { shift: true, ctrl: false }, 10);
    expect(s.selected).toEqual([2, 3, 4, 5]);
    expect(s.anchor).toBe(5);
    s = click(s, 7, { shift: true, ctrl: false }, 10);
    expect(s.selected).toEqual([5, 6, 7]);
  });
  it("ctrl+shift adds the range to the existing selection", () => {
    let s = click(empty, 0, none, 10);
    s = click(s, 5, { shift: false, ctrl: true }, 10);
    s = click(s, 7, { shift: true, ctrl: true }, 10);
    expect(s.selected).toEqual([0, 5, 6, 7]);
  });
  it("ignores out-of-range clicks; selectAll and clamp work", () => {
    expect(click(empty, 12, none, 10)).toBe(empty);
    expect(selectAll(3).selected).toEqual([0, 1, 2]);
    expect(clamp({ selected: [1, 8], anchor: 8 }, 5)).toEqual({ selected: [1], anchor: 1 });
  });
});

describe("moveTarget", () => {
  it("accounts for removed pages before the drop gap", () => {
    // pages 0..5, drag [1,2] into the gap before page 5 -> after removal list has 4 items, they land at 3
    expect(moveTarget([1, 2], 5)).toBe(3);
    expect(moveTarget([4], 0)).toBe(0);
    expect(moveTarget([0], 6)).toBe(5);
  });
  it("returns null for no-op drops", () => {
    expect(moveTarget([2], 2)).toBeNull(); // gap right before itself
    expect(moveTarget([2], 3)).toBeNull(); // gap right after itself
    expect(moveTarget([1, 2], 1)).toBeNull();
    expect(moveTarget([1, 2], 3)).toBeNull();
    expect(moveTarget([], 1)).toBeNull();
  });
  it("non-contiguous selections gather at the drop", () => {
    expect(moveTarget([0, 2], 4)).toBe(2);
  });
});

describe("nudgeTarget", () => {
  it("moves the block one step", () => {
    expect(nudgeTarget([2, 3], -1, 6)).toBe(1);
    expect(nudgeTarget([2, 3], 1, 6)).toBe(3);
    expect(nudgeTarget([0], -1, 6)).toBeNull();
    expect(nudgeTarget([5], 1, 6)).toBeNull();
  });
});

import { dropGap } from "./selection";

describe("dropGap", () => {
  // 2 rows x 3 columns of 100x100 cells, 10px gaps
  const rects = Array.from({ length: 6 }, (_, i) => {
    const c = i % 3, r = Math.floor(i / 3);
    return { left: c * 110, top: r * 110, right: c * 110 + 100, bottom: r * 110 + 100 };
  });
  it("picks the gap by cell midpoint within a row", () => {
    expect(dropGap(rects, 10, 50)).toBe(0);
    expect(dropGap(rects, 70, 50)).toBe(1);
    expect(dropGap(rects, 150, 50)).toBe(1);
    expect(dropGap(rects, 170, 50)).toBe(2);
    expect(dropGap(rects, 320, 50)).toBe(3); // past the last cell of row 1
  });
  it("uses the nearest row when between or outside rows", () => {
    expect(dropGap(rects, 10, 300)).toBe(3);
    expect(dropGap(rects, 320, 300)).toBe(6);
    expect(dropGap(rects, 10, -50)).toBe(0);
    expect(dropGap(rects, 10, 106)).toBe(3);
  });
  it("handles an empty list", () => {
    expect(dropGap([], 5, 5)).toBe(0);
  });
});
