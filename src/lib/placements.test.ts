import { describe, expect, it } from "vitest";
import {
  center, clampToPage, commit, handlePos, hit, initHistory, redo, resizeFromCorner, rotateHandlePos, rotateTo, textBox, undo,
  type Placement,
} from "./placements";

const base: Placement = { id: "a", page: 0, x: 100, y: 100, w: 80, h: 40, rotation: 0, kind: "signature", sigId: "s" };
const near = (a: number, b: number, tol = 1e-6) => expect(Math.abs(a - b)).toBeLessThan(tol);

describe("hit testing", () => {
  it("handles rotation", () => {
    expect(hit(base, 140, 120)).toBe(true);
    expect(hit(base, 200, 120)).toBe(false);
    const r = { ...base, rotation: 90 }; // now 40 wide x 80 tall around (140,120)
    expect(hit(r, 140, 155)).toBe(true);
    expect(hit(r, 175, 120)).toBe(false);
  });
});

describe("handles", () => {
  it("corner positions rotate with the box", () => {
    const p = handlePos(base, "se");
    near(p.x, 180); near(p.y, 140);
    const q = handlePos({ ...base, rotation: 90 }, "se"); // (+40,+20) rotated 90deg cw => (-20,+40)
    near(q.x, 120); near(q.y, 160);
  });
  it("rotate handle sits above the top edge and follows rotation", () => {
    const h = rotateHandlePos(base, 20);
    near(h.x, 140); near(h.y, 100 - 20);
    const r = rotateHandlePos({ ...base, rotation: 90 }, 20); // above becomes to the right
    near(r.x, 140 + 20 + 20); near(r.y, 120);
  });
});

describe("resizeFromCorner", () => {
  it("keeps the opposite corner fixed and the aspect ratio", () => {
    const out = resizeFromCorner(base, "se", { x: 260, y: 100 + 5 });
    near(out.x, 100); near(out.y, 100);
    near(out.w / out.h, 2);
    near(out.w, 160);
  });
  it("opposite corner stays fixed for a rotated box too", () => {
    const p = { ...base, rotation: 30 };
    const anchorBefore = handlePos(p, "nw");
    const target = { x: 300, y: 200 };
    const out = resizeFromCorner(p, "se", target);
    const anchorAfter = handlePos(out, "nw");
    near(anchorAfter.x, anchorBefore.x, 1e-6); near(anchorAfter.y, anchorBefore.y, 1e-6);
    expect(out.w).toBeGreaterThan(p.w);
    near(out.rotation, 30);
  });
  it("free resize changes aspect and never collapses", () => {
    const out = resizeFromCorner(base, "se", { x: 140, y: 400 }, false);
    near(out.w, 40); near(out.h, 300);
    const tiny = resizeFromCorner(base, "se", { x: 90, y: 90 }, false);
    expect(tiny.w).toBeGreaterThanOrEqual(8); expect(tiny.h).toBeGreaterThanOrEqual(8);
  });
  it("works from every corner", () => {
    for (const h of ["nw", "ne", "se", "sw"] as const) {
      const pos = handlePos(base, h);
      const out = resizeFromCorner(base, h, pos); // dragging a handle onto itself is a no-op
      near(out.x, base.x); near(out.y, base.y); near(out.w, base.w); near(out.h, base.h);
    }
  });
});

describe("rotateTo", () => {
  it("points the handle at the pointer and snaps near right angles", () => {
    const c = center(base);
    expect(rotateTo(base, { x: c.x, y: c.y - 100 }).rotation).toBe(0);
    expect(rotateTo(base, { x: c.x + 100, y: c.y }).rotation).toBe(90);
    expect(rotateTo(base, { x: c.x, y: c.y + 100 }).rotation).toBe(180);
    expect(rotateTo(base, { x: c.x - 100, y: c.y }).rotation).toBe(270);
    expect(rotateTo(base, { x: c.x + 100, y: c.y - 100 }).rotation).toBe(45);
    expect(rotateTo(base, { x: c.x + 100, y: c.y - 3 }, false).rotation).not.toBe(90);
  });
});

describe("misc", () => {
  it("clampToPage keeps the centre inside", () => {
    const out = clampToPage({ ...base, x: 1000, y: -500 }, 300, 400);
    const c = center(out);
    expect(c.x).toBe(300); expect(c.y).toBe(0);
  });
  it("textBox measures with padding", () => {
    expect(textBox("abc", 10, (t, s) => t.length * s * 0.5)).toEqual({ w: 19, h: 14 });
  });
});

describe("history", () => {
  it("undo/redo and redo is cleared by a new commit", () => {
    let h = initHistory<number[]>([]);
    h = commit(h, [1]);
    h = commit(h, [1, 2]);
    h = undo(h);
    expect(h.present).toEqual([1]);
    h = redo(h);
    expect(h.present).toEqual([1, 2]);
    h = undo(undo(h));
    expect(h.present).toEqual([]);
    expect(undo(h)).toBe(h);
    h = commit(h, [9]);
    expect(h.future).toEqual([]);
    expect(redo(h)).toBe(h);
  });
});
