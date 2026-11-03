import { useRef } from "react";
import {
  clampToPage, defaultSignatureSize, newId, resizeFromCorner, rotateTo, textBox,
  type Handle, type Placement,
} from "../lib/placements";
import { measureText } from "../lib/measure";
import { useSign } from "../signStore";
import { SigPreview } from "./SigPreview";

interface Props {
  page: number;
  width: number; // page size in points (view space)
  height: number;
  scale: number; // css px per point
}

type DragInit = { kind: "move"; sx: number; sy: number } | { kind: "resize"; handle: Handle } | { kind: "rotate" };
type Drag = DragInit & { start: Placement; before: Placement[] };

const HANDLES: Handle[] = ["nw", "ne", "se", "sw"];
const todayText = () => new Date().toLocaleDateString(undefined, { year: "numeric", month: "2-digit", day: "2-digit" });

/** Interactive overlay for one page: place, select, move, resize, rotate. */
export function SignLayer({ page, width, height, scale }: Props) {
  const { history, selectedId, armed, library } = useSign();
  const root = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const mine = history.present.filter((p) => p.page === page);

  const toPt = (e: { clientX: number; clientY: number }) => {
    const r = root.current!.getBoundingClientRect();
    return { x: (e.clientX - r.left) / scale, y: (e.clientY - r.top) / scale };
  };
  const replace = (p: Placement) => useSign.getState().liveUpdate(useSign.getState().history.present.map((q) => (q.id === p.id ? p : q)));

  const place = (e: React.PointerEvent) => {
    if (!armed) return;
    const pt = toPt(e);
    let p: Placement;
    if (armed.kind === "signature") {
      const sig = library.find((s) => s.id === armed.sigId);
      if (!sig) return;
      const { w, h } = defaultSignatureSize(sig.aspect, width);
      p = { id: newId(), page, x: pt.x - w / 2, y: pt.y - h / 2, w, h, rotation: 0, kind: "signature", sigId: sig.id };
    } else {
      const isDate = armed.kind === "date";
      const text = isDate ? todayText() : "Text";
      const fontSize = 12;
      const b = textBox(text, fontSize, measureText);
      p = { id: newId(), page, x: pt.x - b.w / 2, y: pt.y - b.h / 2, w: b.w, h: b.h, rotation: 0, kind: "text", text, fontSize, isDate };
    }
    p = clampToPage(p, width, height);
    const s = useSign.getState();
    s.commitPlacements([...s.history.present, p]);
    s.select(p.id);
    s.setArmed(null);
  };

  const begin = (e: React.PointerEvent, p: Placement, d: DragInit) => {
    e.stopPropagation();
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    useSign.getState().select(p.id);
    drag.current = { ...d, start: p, before: useSign.getState().history.present };
  };
  const move = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const pt = toPt(e);
    if (d.kind === "move") {
      replace(clampToPage({ ...d.start, x: d.start.x + (pt.x - d.sx), y: d.start.y + (pt.y - d.sy) }, width, height));
    } else if (d.kind === "resize") {
      let out = resizeFromCorner(d.start, d.handle, pt, !e.shiftKey);
      if (out.kind === "text" && d.start.fontSize) out = { ...out, fontSize: Math.max(4, Math.round(d.start.fontSize * (out.h / d.start.h) * 10) / 10) };
      replace(out);
    } else {
      replace(rotateTo(d.start, pt, !e.shiftKey));
    }
  };
  const end = () => {
    const d = drag.current;
    drag.current = null;
    if (d) useSign.getState().endDrag(d.before);
  };

  return (
    <div
      ref={root}
      className="absolute inset-0 select-none"
      style={{ cursor: armed ? "crosshair" : "default", touchAction: "none" }}
      data-sign-layer={page}
      onPointerDown={(e) => { if (armed) place(e); else useSign.getState().select(null); }}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={end}
    >
      {mine.map((p) => {
        const selected = p.id === selectedId;
        const sig = p.kind === "signature" ? library.find((s) => s.id === p.sigId) : undefined;
        return (
          <div
            key={p.id}
            role="button"
            tabIndex={0}
            aria-label={p.kind === "text" ? `Text: ${p.text}` : `Signature ${sig?.name ?? ""}`}
            aria-pressed={selected}
            className={`absolute ${selected ? "outline outline-2 outline-blue-600" : "outline outline-1 outline-transparent hover:outline-blue-300"}`}
            style={{
              left: p.x * scale, top: p.y * scale, width: p.w * scale, height: p.h * scale,
              transform: `rotate(${p.rotation}deg)`, transformOrigin: "center", cursor: armed ? "crosshair" : "move",
            }}
            onPointerDown={(e) => { if (armed) return; begin(e, p, { kind: "move", sx: toPt(e).x, sy: toPt(e).y }); }}
            onKeyDown={(e) => {
              const s = useSign.getState();
              if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); s.select(p.id); s.deleteSelected(); }
              else if (e.key.startsWith("Arrow")) {
                e.preventDefault();
                const step = e.shiftKey ? 10 : 1;
                const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
                const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
                s.commitPlacements(s.history.present.map((q) => (q.id === p.id ? clampToPage({ ...q, x: q.x + dx, y: q.y + dy }, width, height) : q)));
              } else if (e.key === "Enter" || e.key === " ") { e.preventDefault(); s.select(p.id); }
            }}
            onFocus={() => useSign.getState().select(p.id)}
          >
            {p.kind === "signature" && sig && <SigPreview sig={sig} className="pointer-events-none h-full w-full" />}
            {p.kind === "text" && (
              <span className="pointer-events-none block select-none whitespace-pre text-black" style={{ fontFamily: "Helvetica, Arial, sans-serif", fontSize: (p.fontSize ?? 12) * scale, lineHeight: `${p.h * scale}px` }}>
                {p.text}
              </span>
            )}
            {selected && (
              <>
                {HANDLES.map((h) => {
                  const left = h.includes("w") ? 0 : p.w * scale;
                  const top = h.includes("n") ? 0 : p.h * scale;
                  return (
                    <div
                      key={h}
                      aria-hidden
                      className="absolute h-3 w-3 -translate-x-1/2 -translate-y-1/2 rounded-sm border border-[var(--accent)] bg-white"
                      style={{ left, top, cursor: h === "nw" || h === "se" ? "nwse-resize" : "nesw-resize" }}
                      onPointerDown={(e) => begin(e, p, { kind: "resize", handle: h })}
                    />
                  );
                })}
                <div aria-hidden className="absolute left-1/2 -top-[22px] h-[22px] w-px bg-[var(--accent)]" style={{ transform: "translateX(-50%)" }} />
                <div
                  aria-hidden
                  title="Rotate"
                  className="absolute left-1/2 h-3 w-3 -translate-x-1/2 rounded-full border border-[var(--accent)] bg-white"
                  style={{ top: -22 * 1 - 6, cursor: "grab" }}
                  onPointerDown={(e) => begin(e, p, { kind: "rotate" })}
                />
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}
