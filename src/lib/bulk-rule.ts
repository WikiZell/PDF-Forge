/** Form <-> rule-file conversion for bulk signing (format documented in docs/bulk-rule-format.md). */

export type PagesField = number | "first" | "last" | "all";

export type PositionField =
  | { type: "fixed"; x: number; y: number }
  | { type: "anchor"; text: string; matchCase: boolean; occurrence: "first" | "last"; side: "right" | "below" | "same"; offsetX: number; offsetY: number };

export type StampField =
  | { type: "date"; format: "iso" | "dmy" | "mdy"; fontSize: number; offsetX: number; offsetY: number }
  | { type: "text"; text: string; fontSize: number; offsetX: number; offsetY: number };

export interface RuleFile {
  version: 1;
  signature: string;
  width: number;
  height: number;
  pages: PagesField;
  position: PositionField;
  stamp?: StampField | null;
  flatten: boolean;
}

export interface RuleForm {
  signature: string;
  width: number;
  height: number;
  pages: "first" | "last" | "all" | "number";
  pageNumber: number;
  position: "fixed" | "anchor";
  x: number;
  y: number;
  anchorText: string;
  matchCase: boolean;
  occurrence: "first" | "last";
  side: "right" | "below" | "same";
  offsetX: number;
  offsetY: number;
  stamp: "none" | "date" | "text";
  dateFormat: "iso" | "dmy" | "mdy";
  stampText: string;
  stampFontSize: number;
  stampOffsetX: number;
  stampOffsetY: number;
  flatten: boolean;
}

export const defaultForm: RuleForm = {
  signature: "", width: 150, height: 50, pages: "last", pageNumber: 1,
  position: "anchor", x: 72, y: 650, anchorText: "Signature:", matchCase: false, occurrence: "last", side: "right", offsetX: 8, offsetY: -10,
  stamp: "none", dateFormat: "iso", stampText: "Approved", stampFontSize: 11, stampOffsetX: 0, stampOffsetY: 54,
  flatten: false,
};

const num = (n: number, fallback: number) => (Number.isFinite(n) ? n : fallback);

/** Returns the rule, or a message describing what is wrong with the form. */
export function formToRule(f: RuleForm): { rule: RuleFile } | { error: string } {
  if (!f.signature) return { error: "Choose a signature." };
  if (!(f.width >= 4 && f.width <= 2000 && f.height >= 4 && f.height <= 2000)) return { error: "Signature width and height must be between 4 and 2000 points." };
  if (f.pages === "number" && !(Number.isInteger(f.pageNumber) && f.pageNumber >= 1)) return { error: "Enter a page number of 1 or more." };
  if (f.position === "anchor" && !f.anchorText.trim()) return { error: "Enter the anchor text to search for." };
  if (f.stamp === "text" && !f.stampText.trim()) return { error: "Enter the stamp text." };
  if (f.stamp !== "none" && !(f.stampFontSize >= 4 && f.stampFontSize <= 200)) return { error: "Stamp font size must be between 4 and 200." };
  const position: PositionField =
    f.position === "fixed"
      ? { type: "fixed", x: num(f.x, 0), y: num(f.y, 0) }
      : { type: "anchor", text: f.anchorText, matchCase: f.matchCase, occurrence: f.occurrence, side: f.side, offsetX: num(f.offsetX, 0), offsetY: num(f.offsetY, 0) };
  let stamp: StampField | null = null;
  if (f.stamp === "date") stamp = { type: "date", format: f.dateFormat, fontSize: f.stampFontSize, offsetX: num(f.stampOffsetX, 0), offsetY: num(f.stampOffsetY, 0) };
  if (f.stamp === "text") stamp = { type: "text", text: f.stampText, fontSize: f.stampFontSize, offsetX: num(f.stampOffsetX, 0), offsetY: num(f.stampOffsetY, 0) };
  return {
    rule: {
      version: 1, signature: f.signature, width: f.width, height: f.height,
      pages: f.pages === "number" ? f.pageNumber : f.pages,
      position, stamp, flatten: f.flatten,
    },
  };
}

export function ruleToForm(r: RuleFile): RuleForm {
  const f: RuleForm = { ...defaultForm, signature: r.signature, width: r.width, height: r.height, flatten: r.flatten };
  if (typeof r.pages === "number") { f.pages = "number"; f.pageNumber = r.pages; } else f.pages = r.pages;
  if (r.position.type === "fixed") {
    f.position = "fixed"; f.x = r.position.x; f.y = r.position.y;
  } else {
    f.position = "anchor"; f.anchorText = r.position.text; f.matchCase = r.position.matchCase;
    f.occurrence = r.position.occurrence; f.side = r.position.side; f.offsetX = r.position.offsetX; f.offsetY = r.position.offsetY;
  }
  if (r.stamp?.type === "date") {
    f.stamp = "date"; f.dateFormat = r.stamp.format; f.stampFontSize = r.stamp.fontSize; f.stampOffsetX = r.stamp.offsetX; f.stampOffsetY = r.stamp.offsetY;
  } else if (r.stamp?.type === "text") {
    f.stamp = "text"; f.stampText = r.stamp.text; f.stampFontSize = r.stamp.fontSize; f.stampOffsetX = r.stamp.offsetX; f.stampOffsetY = r.stamp.offsetY;
  }
  return f;
}

export interface ItemDto {
  input: string;
  status: "signed" | "skipped" | "failed";
  detail: string;
  output: string | null;
}

export function summarize(items: ItemDto[]) {
  const c = { signed: 0, skipped: 0, failed: 0 };
  for (const i of items) c[i.status]++;
  return c;
}
