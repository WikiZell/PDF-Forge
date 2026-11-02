import { describe, expect, it } from "vitest";
import { groupAtScroll, groupIndexOfPage, groupPages, referenceSize } from "./layout";
import { parseZoom, serializeZoom } from "./zoom";

describe("layout", () => {
  it("groups pages for each layout", () => {
    expect(groupPages("single", 3)).toEqual([[0], [1], [2]]);
    expect(groupPages("continuous", 2)).toEqual([[0], [1]]);
    expect(groupPages("two-page", 5)).toEqual([[0, 1], [2, 3], [4]]);
    expect(groupPages("two-page", 0)).toEqual([]);
  });
  it("finds the group containing a page", () => {
    expect(groupIndexOfPage("two-page", 3)).toBe(1);
    expect(groupIndexOfPage("single", 3)).toBe(3);
  });
  it("reference size: widest page in continuous, summed pair in two-page", () => {
    const sizes = [{ width: 100, height: 200 }, { width: 150, height: 120 }];
    expect(referenceSize("continuous", sizes, 0)).toEqual({ width: 150, height: 200 });
    expect(referenceSize("two-page", sizes, 1)).toEqual({ width: 262, height: 200 });
    expect(referenceSize("single", sizes, 1)).toEqual({ width: 150, height: 120 });
    expect(referenceSize("single", [], 0)).toBeNull();
  });
  it("groupAtScroll picks the first group still on screen", () => {
    const tops = [0, 100, 200];
    const heights = [90, 90, 90];
    expect(groupAtScroll(tops, heights, 0)).toBe(0);
    expect(groupAtScroll(tops, heights, 95)).toBe(1);
    expect(groupAtScroll(tops, heights, 10_000)).toBe(2);
  });
});

describe("zoom serialisation", () => {
  it("round-trips and sanitises", () => {
    for (const z of [{ kind: "fit-width" }, { kind: "fit-page" }, { kind: "percent", value: 175 }] as const) {
      expect(parseZoom(serializeZoom(z))).toEqual(z);
    }
    expect(parseZoom("garbage")).toEqual({ kind: "fit-width" });
    expect(parseZoom("9999")).toEqual({ kind: "fit-width" });
    expect(parseZoom("5")).toEqual({ kind: "percent", value: 25 });
  });
});
