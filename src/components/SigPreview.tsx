import { useSigImage } from "../hooks/useSigImage";
import type { SigMeta } from "../lib/sig-api";

/** Visual for a library signature: SVG for drawn signatures, the stored PNG for the rest. */
export function SigPreview({ sig, className = "", fit = false }: { sig: SigMeta; className?: string; fit?: boolean }) {
  const url = useSigImage(sig.id, !sig.drawn);
  if (sig.drawn) {
    const d = sig.drawn;
    const col = `rgb(${d.color.map((c) => Math.round(c * 255)).join(",")})`;
    return (
      <svg viewBox={`0 0 ${d.view[0]} ${d.view[1]}`} className={className} preserveAspectRatio={fit ? "xMidYMid meet" : "none"} role="img" aria-label={sig.name}>
        {d.strokes.map((s, i) => (
          <polyline
            key={i}
            points={s.map((p) => `${p[0]},${p[1]}`).join(" ")}
            fill="none"
            stroke={col}
            strokeWidth={d.width}
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        ))}
      </svg>
    );
  }
  return url ? <img src={url} alt={sig.name} className={className} draggable={false} style={{ objectFit: fit ? "contain" : "fill" }} /> : <div className={className} />;
}
