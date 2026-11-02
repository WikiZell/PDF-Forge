let ctx: CanvasRenderingContext2D | null | undefined;

/** Width in points of `text` in Helvetica/Arial at `size` points (browser metrics approximate the PDF font). */
export function measureText(text: string, size: number): number {
  if (ctx === undefined) {
    try {
      ctx = document.createElement("canvas").getContext("2d");
    } catch {
      ctx = null;
    }
  }
  if (!ctx) return text.length * size * 0.55;
  ctx.font = `${size}px Helvetica, Arial, sans-serif`;
  return ctx.measureText(text).width;
}
