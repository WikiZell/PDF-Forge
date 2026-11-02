import { describe, expect, it } from "vitest";
import { defaultForm, formToRule, ruleToForm, summarize, type RuleForm } from "./bulk-rule";

const ok = (f: RuleForm) => {
  const r = formToRule(f);
  if ("error" in r) throw new Error(r.error);
  return r.rule;
};
const base: RuleForm = { ...defaultForm, signature: "abc" };

describe("formToRule", () => {
  it("builds anchor rules, with and without a stamp", () => {
    const r = ok(base);
    expect(r.position).toMatchObject({ type: "anchor", text: "Signature:", side: "right", occurrence: "last" });
    expect(r.pages).toBe("last");
    expect(r.stamp).toBeNull();
    const s = ok({ ...base, stamp: "date", dateFormat: "dmy" });
    expect(s.stamp).toMatchObject({ type: "date", format: "dmy", fontSize: 11 });
  });
  it("builds fixed rules and numeric pages", () => {
    const r = ok({ ...base, position: "fixed", x: 10, y: 20, pages: "number", pageNumber: 3 });
    expect(r.position).toEqual({ type: "fixed", x: 10, y: 20 });
    expect(r.pages).toBe(3);
  });
  it("reports clear errors", () => {
    expect("error" in formToRule({ ...base, signature: "" })).toBe(true);
    expect("error" in formToRule({ ...base, width: 2 })).toBe(true);
    expect("error" in formToRule({ ...base, anchorText: "  " })).toBe(true);
    expect("error" in formToRule({ ...base, pages: "number", pageNumber: 0 })).toBe(true);
    expect("error" in formToRule({ ...base, stamp: "text", stampText: "" })).toBe(true);
    expect("error" in formToRule({ ...base, stamp: "date", stampFontSize: 1 })).toBe(true);
  });
  it("replaces NaN offsets with 0", () => {
    const r = ok({ ...base, offsetX: NaN });
    expect(r.position).toMatchObject({ offsetX: 0 });
  });
});

describe("ruleToForm", () => {
  it("round-trips every variant", () => {
    for (const f of [
      base,
      { ...base, position: "fixed" as const, x: 1, y: 2, pages: "all" as const },
      { ...base, pages: "number" as const, pageNumber: 4, stamp: "text" as const, stampText: "OK", flatten: true },
      { ...base, stamp: "date" as const, dateFormat: "mdy" as const, side: "below" as const, occurrence: "first" as const, matchCase: true },
    ]) {
      const back = ruleToForm(ok(f));
      expect(ok(back)).toEqual(ok(f));
    }
  });
});

describe("summarize", () => {
  it("counts statuses", () => {
    const i = (status: "signed" | "skipped" | "failed") => ({ input: "x", status, detail: "", output: null });
    expect(summarize([i("signed"), i("signed"), i("failed"), i("skipped")])).toEqual({ signed: 2, skipped: 1, failed: 1 });
  });
});
