export type ZoomMode =
  | { kind: "fit-width" }
  | { kind: "fit-page" }
  | { kind: "percent"; value: number };

export const MIN_ZOOM = 25;
export const MAX_ZOOM = 400;

export interface Size {
  width: number;
  height: number;
}

export function clampPercent(p: number): number {
  if (!Number.isFinite(p)) return 100;
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.round(p)));
}

/** Scale factor (CSS px per PDF point) for a zoom mode. `padding` is the viewport margin in px. */
export function computeScale(mode: ZoomMode, page: Size, viewport: Size, padding = 32): number {
  const availW = Math.max(1, viewport.width - padding);
  const availH = Math.max(1, viewport.height - padding);
  switch (mode.kind) {
    case "fit-width":
      return clampScale(availW / page.width);
    case "fit-page":
      return clampScale(Math.min(availW / page.width, availH / page.height));
    case "percent":
      return clampPercent(mode.value) / 100;
  }
}

function clampScale(s: number): number {
  return Math.min(MAX_ZOOM / 100, Math.max(MIN_ZOOM / 100, s));
}

/** Step to the next preset zoom level above/below the current percent. */
export function stepZoom(currentPercent: number, dir: 1 | -1): number {
  const levels = [25, 50, 75, 100, 125, 150, 200, 300, 400];
  const cur = Math.round(currentPercent);
  if (dir > 0) return levels.find((l) => l > cur) ?? MAX_ZOOM;
  return [...levels].reverse().find((l) => l < cur) ?? MIN_ZOOM;
}

export function serializeZoom(z: ZoomMode): string {
  return z.kind === "percent" ? String(clampPercent(z.value)) : z.kind;
}

export function parseZoom(s: string | undefined | null): ZoomMode {
  if (s === "fit-page") return { kind: "fit-page" };
  if (s && /^\d{1,3}$/.test(s)) return { kind: "percent", value: clampPercent(Number(s)) };
  return { kind: "fit-width" };
}
