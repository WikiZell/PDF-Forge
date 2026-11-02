import { describe, expect, it } from "vitest";
import { clampPercent, computeScale, stepZoom } from "./zoom";

const page = { width: 600, height: 800 };
const vp = { width: 1032, height: 832 };

describe("zoom", () => {
  it("fit width uses viewport width minus padding", () => {
    expect(computeScale({ kind: "fit-width" }, page, vp)).toBeCloseTo(1000 / 600);
  });
  it("fit page is limited by the tighter dimension", () => {
    expect(computeScale({ kind: "fit-page" }, page, vp)).toBeCloseTo(800 / 800);
  });
  it("percent mode is clamped to 25-400", () => {
    expect(computeScale({ kind: "percent", value: 10 }, page, vp)).toBe(0.25);
    expect(computeScale({ kind: "percent", value: 9999 }, page, vp)).toBe(4);
  });
  it("clampPercent handles NaN", () => {
    expect(clampPercent(NaN)).toBe(100);
  });
  it("stepZoom walks preset levels and stops at the ends", () => {
    expect(stepZoom(100, 1)).toBe(125);
    expect(stepZoom(100, -1)).toBe(75);
    expect(stepZoom(400, 1)).toBe(400);
    expect(stepZoom(25, -1)).toBe(25);
  });
});
