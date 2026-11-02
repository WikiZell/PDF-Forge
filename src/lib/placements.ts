/** Geometry for placed signatures/text. All units are PDF points in *view space* (top-left origin). */

export type Handle = "nw" | "ne" | "se" | "sw";

export interface Placement {
  id: string;
  page: number; // 0-based
  x: number; // top-left of the unrotated box
  y: number;
  w: number;
  h: number;
  rotation: number; // degrees clockwise about the centre
  kind: "signature" | "text";
  sigId?: string;
  text?: string;
  fontSize?: number;
  isDate?: boolean;
}

export const MIN_SIZE = 8;

export const center = (p: Pick<Placement, "x" | "y" | "w" | "h">) => ({ x: p.x + p.w / 2, y: p.y + p.h / 2 });

const rad = (d: number) => (d * Math.PI) / 180;

/** Rotate (dx, dy) by `deg` clockwise on a y-down plane. */
export function rot(dx: number, dy: number, deg: number) {
  const c = Math.cos(rad(deg));
  const s = Math.sin(rad(deg));
  return { x: dx * c - dy * s, y: dx * s + dy * c };
}

/** Pointer in view space -> coordinates relative to the box centre in the box's own (unrotated) frame. */
export function toLocal(p: Placement, x: number, y: number) {
  const c = center(p);
  return rot(x - c.x, y - c.y, -p.rotation);
}

export function hit(p: Placement, x: number, y: number): boolean {
  const l = toLocal(p, x, y);
  return Math.abs(l.x) <= p.w / 2 && Math.abs(l.y) <= p.h / 2;
}

const SIGN: Record<Handle, [number, number]> = { nw: [-1, -1], ne: [1, -1], se: [1, 1], sw: [-1, 1] };

/** View-space position of a corner handle. */
export function handlePos(p: Placement, h: Handle) {
  const [sx, sy] = SIGN[h];
  const c = center(p);
  const r = rot((sx * p.w) / 2, (sy * p.h) / 2, p.rotation);
  return { x: c.x + r.x, y: c.y + r.y };
}

/** Position of the rotate handle, `gap` points beyond the top edge. */
export function rotateHandlePos(p: Placement, gap = 22) {
  const c = center(p);
  const r = rot(0, -(p.h / 2 + gap), p.rotation);
  return { x: c.x + r.x, y: c.y + r.y };
}

/** Drag a corner handle to `ptr`; the opposite corner stays fixed. */
export function resizeFromCorner(p: Placement, handle: Handle, ptr: { x: number; y: number }, keepAspect = true): Placement {
  const [sx, sy] = SIGN[handle];
  const l = toLocal(p, ptr.x, ptr.y);
  const ax = (-sx * p.w) / 2;
  const ay = (-sy * p.h) / 2;
  let nw = Math.max(MIN_SIZE, (l.x - ax) * sx);
  let nh = Math.max(MIN_SIZE, (l.y - ay) * sy);
  if (keepAspect) {
    const s = Math.max(nw / p.w, nh / p.h);
    nw = Math.max(MIN_SIZE, p.w * s);
    nh = Math.max(MIN_SIZE, p.h * (nw / p.w));
  }
  const newCenterLocal = { x: ax + (sx * nw) / 2, y: ay + (sy * nh) / 2 };
  const c0 = center(p);
  const r = rot(newCenterLocal.x, newCenterLocal.y, p.rotation);
  const cx = c0.x + r.x;
  const cy = c0.y + r.y;
  return { ...p, w: nw, h: nh, x: cx - nw / 2, y: cy - nh / 2 };
}

/** Rotation that points the rotate handle at `ptr`. Snaps to multiples of 90 within 4 degrees. */
export function rotateTo(p: Placement, ptr: { x: number; y: number }, snap = true): Placement {
  const c = center(p);
  let deg = (Math.atan2(ptr.y - c.y, ptr.x - c.x) * 180) / Math.PI + 90;
  deg = ((deg % 360) + 360) % 360;
  if (snap) {
    const nearest = Math.round(deg / 90) * 90;
    if (Math.abs(deg - nearest) < 4) deg = nearest % 360;
  }
  return { ...p, rotation: Math.round(deg * 10) / 10 };
}

/** Keep at least `keep` points of the box centre inside the page. */
export function clampToPage(p: Placement, pageW: number, pageH: number): Placement {
  const c = center(p);
  const cx = Math.min(pageW, Math.max(0, c.x));
  const cy = Math.min(pageH, Math.max(0, c.y));
  return { ...p, x: p.x + (cx - c.x), y: p.y + (cy - c.y) };
}

/** Default size for a freshly placed signature: ~30% of the page width, bounded. */
export function defaultSignatureSize(aspect: number, pageW: number) {
  const w = Math.min(220, Math.max(70, pageW * 0.3));
  return { w, h: w / (aspect > 0 ? aspect : 3) };
}

/** Box for a text placement given a width measurer. */
export function textBox(text: string, fontSize: number, measure: (t: string, size: number) => number) {
  return { w: Math.max(MIN_SIZE, Math.ceil(measure(text, fontSize)) + 4), h: Math.ceil(fontSize * 1.4) };
}

export const newId = () => Math.random().toString(36).slice(2, 10) + Date.now().toString(36).slice(-4);

/** Undo/redo history of placement lists. */
export interface History<T> {
  past: T[];
  present: T;
  future: T[];
}
export const MAX_HISTORY = 200;
export const initHistory = <T,>(present: T): History<T> => ({ past: [], present, future: [] });
export function commit<T>(h: History<T>, next: T): History<T> {
  return { past: [...h.past, h.present].slice(-MAX_HISTORY), present: next, future: [] };
}
export function undo<T>(h: History<T>): History<T> {
  if (h.past.length === 0) return h;
  return { past: h.past.slice(0, -1), present: h.past[h.past.length - 1], future: [h.present, ...h.future] };
}
export function redo<T>(h: History<T>): History<T> {
  if (h.future.length === 0) return h;
  return { past: [...h.past, h.present], present: h.future[0], future: h.future.slice(1) };
}
