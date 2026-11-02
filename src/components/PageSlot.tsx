import { memo, useEffect, useMemo, useState } from "react";
import { pageText, type SearchHit, type TextRun } from "../lib/api";
import { useNear } from "../hooks/useNear";
import { usePageImage } from "../hooks/usePageImage";
import { useSign } from "../signStore";
import { SignLayer } from "./SignLayer";

interface Props {
  docId: number;
  page: number;
  width: number; // PDF points
  height: number;
  scale: number; // CSS px per point
  hits: { hit: SearchHit; index: number }[];
  currentHit: number;
}

let measureCtx: CanvasRenderingContext2D | null = null;
function textWidth(text: string, px: number): number {
  measureCtx ??= document.createElement("canvas").getContext("2d");
  if (!measureCtx) return text.length * px * 0.5;
  measureCtx.font = `${px}px sans-serif`;
  return measureCtx.measureText(text).width;
}

/** One page: bitmap + invisible selectable text layer + search highlights. */
export const PageSlot = memo(function PageSlot({ docId, page, width, height, scale, hits, currentHit }: Props) {
  const [ref, near] = useNear<HTMLDivElement>();
  const dpr = window.devicePixelRatio || 1;
  const renderScale = Math.round(scale * dpr * 100) / 100;
  const url = usePageImage(docId, page, renderScale, near);
  const [runs, setRuns] = useState<TextRun[] | null>(null);
  const signing = useSign((s) => s.active);

  useEffect(() => {
    if (!near || runs) return;
    let alive = true;
    pageText(docId, page).then((r) => alive && setRuns(r)).catch(() => alive && setRuns([]));
    return () => {
      alive = false;
    };
  }, [near, runs, docId, page]);

  const spans = useMemo(
    () =>
      (runs ?? []).map((r) => {
        const px = Math.max(1, r.rect.h * scale * 0.9);
        const natural = textWidth(r.text, px);
        const target = r.rect.w * scale;
        return { r, px, sx: natural > 0 ? target / natural : 1 };
      }),
    [runs, scale],
  );

  const w = Math.round(width * scale);
  const h = Math.round(height * scale);
  return (
    <div
      ref={ref}
      data-page={page}
      role="img"
      aria-label={`Page ${page + 1}`}
      className="relative shrink-0 bg-white shadow-lg"
      style={{ width: w, height: h }}
    >
      {url && <img src={url} alt="" width={w} height={h} draggable={false} className="absolute inset-0 select-none" />}
      {hits.map(({ hit, index }) =>
        hit.rects.map((r, i) => (
          <div
            key={`${index}-${i}`}
            aria-hidden
            className={index === currentHit ? "absolute bg-orange-500/50" : "absolute bg-yellow-300/50"}
            style={{ left: r.x * scale, top: r.y * scale, width: r.w * scale, height: r.h * scale }}
          />
        )),
      )}
      {signing && <SignLayer page={page} width={width} height={height} scale={scale} />}
      <div className={`absolute inset-0 overflow-hidden ${signing ? "pointer-events-none" : ""}`} lang="">
        {spans.map(({ r, px, sx }, i) => (
          <span
            key={i}
            className="absolute whitespace-pre text-transparent selection:bg-blue-500/40"
            style={{
              left: r.rect.x * scale,
              top: r.rect.y * scale,
              fontSize: px,
              lineHeight: `${r.rect.h * scale}px`,
              transform: `scaleX(${sx})`,
              transformOrigin: "0 0",
            }}
          >
            {r.text}
          </span>
        ))}
      </div>
    </div>
  );
});
