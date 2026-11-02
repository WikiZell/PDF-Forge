import { useEffect, useRef, useState } from "react";
import { toAppError } from "../lib/api";
import * as api from "../lib/sig-api";
import { useSign } from "../signStore";

type Tab = "draw" | "type" | "import";
type Pt = [number, number, number];

const DRAW_W = 560;
const DRAW_H = 200;
const COLORS: { name: string; rgb: [number, number, number] }[] = [
  { name: "Black", rgb: [0.05, 0.05, 0.05] },
  { name: "Blue", rgb: [0.05, 0.2, 0.7] },
];

export function SignatureDialog({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("draw");
  const [name, setName] = useState("My signature");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const { addSignature } = useSign.getState();
  const finish = (m: api.SigMeta) => {
    addSignature(m);
    onClose();
  };
  const run = async (fn: () => Promise<api.SigMeta>) => {
    setSaving(true);
    setError(null);
    try {
      finish(await fn());
    } catch (e) {
      setError(toAppError(e).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-20 grid place-items-center bg-black/40" role="presentation">
      <div
        role="dialog" aria-modal="true" aria-labelledby="sigdlg-h"
        className="max-h-[90vh] w-[40rem] max-w-[95vw] overflow-auto rounded-lg bg-white p-5 shadow-xl dark:bg-neutral-800"
        onKeyDown={(e) => e.key === "Escape" && onClose()}
      >
        <h2 id="sigdlg-h" className="mb-3 text-lg font-semibold">New signature</h2>
        <div role="tablist" aria-label="Signature source" className="mb-3 flex gap-1 border-b">
          {(["draw", "type", "import"] as Tab[]).map((t) => (
            <button key={t} role="tab" aria-selected={tab === t} onClick={() => setTab(t)}
              className={`px-3 py-1.5 text-sm ${tab === t ? "border-b-2 border-blue-600 font-semibold" : ""}`}>
              {t === "draw" ? "Draw" : t === "type" ? "Type" : "Import image"}
            </button>
          ))}
        </div>
        <label className="mb-3 block text-sm">
          Name
          <input value={name} onChange={(e) => setName(e.target.value)} maxLength={60} className="ml-2 w-64 rounded border px-2 py-1 dark:bg-neutral-900" />
        </label>

        {tab === "draw" && <DrawTab name={name} saving={saving} onSave={(d) => run(() => api.sigCreateDrawn({ name, ...d }))} />}
        {tab === "type" && <TypeTab name={name} saving={saving} onSave={(png, text, font) => run(() => api.sigCreateImage(png, { name, kind: "typed", text, font }))} />}
        {tab === "import" && <ImportTab name={name} saving={saving} onSave={(png) => run(() => api.sigCreateImage(png, { name, kind: "imported" }))} onError={setError} />}

        {error && <p role="alert" className="mt-2 text-sm text-red-600">{error}</p>}
        <div className="mt-4 flex justify-end">
          <button className="rounded border px-3 py-1" onClick={onClose}>Cancel</button>
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- draw

function DrawTab({ name, saving, onSave }: { name: string; saving: boolean; onSave: (d: api.Drawn) => void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const strokes = useRef<Pt[][]>([]);
  const [count, setCount] = useState(0);
  const [color, setColor] = useState(0);
  const [width, setWidth] = useState(3);
  const drawing = useRef(false);

  const redraw = () => {
    const c = canvas.current;
    const ctx = c?.getContext("2d");
    if (!c || !ctx) return;
    ctx.clearRect(0, 0, c.width, c.height);
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.strokeStyle = `rgb(${COLORS[color].rgb.map((v) => Math.round(v * 255)).join(",")})`;
    for (const s of strokes.current) {
      if (s.length === 1) {
        ctx.lineWidth = width;
        ctx.beginPath(); ctx.moveTo(s[0][0], s[0][1]); ctx.lineTo(s[0][0] + 0.01, s[0][1]); ctx.stroke();
      }
      for (let i = 1; i < s.length; i++) {
        ctx.lineWidth = width * (0.45 + 1.1 * ((s[i - 1][2] + s[i][2]) / 2));
        ctx.beginPath(); ctx.moveTo(s[i - 1][0], s[i - 1][1]); ctx.lineTo(s[i][0], s[i][1]); ctx.stroke();
      }
    }
  };
  useEffect(redraw); // eslint-disable-line react-hooks/exhaustive-deps

  const pos = (e: React.PointerEvent): Pt => {
    const r = canvas.current!.getBoundingClientRect();
    // `pressure` is 0.5 for mice while a button is down and varies for pens.
    return [((e.clientX - r.left) / r.width) * DRAW_W, ((e.clientY - r.top) / r.height) * DRAW_H, e.pressure > 0 ? e.pressure : 0.5];
  };

  const save = () => {
    const all = strokes.current.flat();
    if (all.length === 0) return;
    const m = width * 2;
    const x0 = Math.min(...all.map((p) => p[0])) - m, y0 = Math.min(...all.map((p) => p[1])) - m;
    const x1 = Math.max(...all.map((p) => p[0])) + m, y1 = Math.max(...all.map((p) => p[1])) + m;
    onSave({
      strokes: strokes.current.map((s) => s.map((p) => [round(p[0] - x0), round(p[1] - y0), round(p[2])] as Pt)),
      view: [round(x1 - x0), round(y1 - y0)],
      width,
      color: COLORS[color].rgb,
    });
  };

  return (
    <div>
      <canvas
        ref={canvas} width={DRAW_W} height={DRAW_H}
        aria-label="Signature drawing area. Draw with a mouse, finger or pen."
        className="w-full touch-none rounded border bg-white"
        style={{ cursor: "crosshair" }}
        onPointerDown={(e) => { canvas.current!.setPointerCapture(e.pointerId); drawing.current = true; strokes.current.push([pos(e)]); redraw(); }}
        onPointerMove={(e) => { if (!drawing.current) return; strokes.current[strokes.current.length - 1].push(pos(e)); redraw(); }}
        onPointerUp={() => { drawing.current = false; setCount(strokes.current.length); }}
        onPointerCancel={() => { drawing.current = false; setCount(strokes.current.length); }}
      />
      <div className="mt-2 flex flex-wrap items-center gap-2 text-sm">
        <button className="rounded border px-2 py-1" disabled={count === 0} onClick={() => { strokes.current.pop(); setCount(strokes.current.length); redraw(); }}>Undo stroke</button>
        <button className="rounded border px-2 py-1" disabled={count === 0} onClick={() => { strokes.current = []; setCount(0); redraw(); }}>Clear</button>
        <label>Ink <select value={color} onChange={(e) => setColor(Number(e.target.value))} className="rounded border px-1 dark:bg-neutral-900">{COLORS.map((c, i) => <option key={c.name} value={i}>{c.name}</option>)}</select></label>
        <label>Thickness <input type="range" min={1} max={8} step={0.5} value={width} onChange={(e) => setWidth(Number(e.target.value))} /></label>
        <span className="flex-1" />
        <button className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-40" disabled={count === 0 || saving || !name.trim()} onClick={save}>Save signature</button>
      </div>
    </div>
  );
}

const round = (n: number) => Math.round(n * 100) / 100;

// ---------------------------------------------------------------- type

function TypeTab({ name, saving, onSave }: { name: string; saving: boolean; onSave: (png: Uint8Array, text: string, font: string) => void }) {
  const [text, setText] = useState("");
  const [font, setFont] = useState<string>(api.SIGNATURE_FONTS[0].id);
  const [color, setColor] = useState(0);
  const canvas = useRef<HTMLCanvasElement>(null);
  const css = api.SIGNATURE_FONTS.find((f) => f.id === font)!.css;

  useEffect(() => {
    let alive = true;
    (async () => {
      try { await document.fonts.load(`80px ${css}`, text || "Ag"); } catch { /* fall back to default */ }
      if (!alive) return;
      const c = canvas.current, ctx = c?.getContext("2d");
      if (!c || !ctx) return;
      ctx.clearRect(0, 0, c.width, c.height);
      ctx.fillStyle = `rgb(${COLORS[color].rgb.map((v) => Math.round(v * 255)).join(",")})`;
      ctx.textBaseline = "middle";
      let size = 110;
      ctx.font = `${size}px ${css}`;
      const w = ctx.measureText(text || "Your name").width;
      if (w > c.width - 40) size = Math.floor((size * (c.width - 40)) / w);
      ctx.font = `${size}px ${css}`;
      ctx.fillText(text || "Your name", 20, c.height / 2);
      ctx.globalAlpha = 1;
    })();
    return () => { alive = false; };
  }, [text, css, color]);

  const save = () => {
    canvas.current!.toBlob(async (b) => {
      if (b) onSave(new Uint8Array(await b.arrayBuffer()), text, font);
    }, "image/png");
  };

  return (
    <div>
      <label className="block text-sm">Type your signature
        <input value={text} onChange={(e) => setText(e.target.value.slice(0, 60))} className="mt-1 w-full rounded border px-2 py-1 dark:bg-neutral-900" />
      </label>
      <div className="mt-2 flex flex-wrap gap-2" role="radiogroup" aria-label="Handwriting font">
        {api.SIGNATURE_FONTS.map((f) => (
          <button key={f.id} role="radio" aria-checked={font === f.id} onClick={() => setFont(f.id)}
            className={`rounded border px-3 py-1 text-2xl ${font === f.id ? "ring-2 ring-blue-600" : ""}`} style={{ fontFamily: f.css }}>
            {text || "Signature"}
          </button>
        ))}
      </div>
      <canvas ref={canvas} width={900} height={220} className="mt-3 w-full rounded border bg-white" aria-label="Signature preview" />
      <div className="mt-2 flex items-center gap-2 text-sm">
        <label>Ink <select value={color} onChange={(e) => setColor(Number(e.target.value))} className="rounded border px-1 dark:bg-neutral-900">{COLORS.map((c, i) => <option key={c.name} value={i}>{c.name}</option>)}</select></label>
        <span className="flex-1" />
        <button className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-40" disabled={!text.trim() || saving || !name.trim()} onClick={save}>Save signature</button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- import

function ImportTab({ name, saving, onSave, onError }: { name: string; saving: boolean; onSave: (png: Uint8Array) => void; onError: (m: string | null) => void }) {
  const [src, setSrc] = useState<Uint8Array | null>(null);
  const [threshold, setThreshold] = useState(200);
  const [preview, setPreview] = useState<{ url: string; bytes: Uint8Array } | null>(null);

  useEffect(() => {
    if (!src) return;
    let alive = true;
    const t = setTimeout(async () => {
      try {
        const blob = await api.sigProcessImport(src, threshold);
        const bytes = new Uint8Array(await blob.arrayBuffer());
        if (!alive) return;
        onError(null);
        setPreview((old) => (old && URL.revokeObjectURL(old.url), { url: URL.createObjectURL(blob), bytes }));
      } catch (e) {
        if (alive) { setPreview(null); onError(toAppError(e).message); }
      }
    }, 120);
    return () => { alive = false; clearTimeout(t); };
  }, [src, threshold, onError]);

  return (
    <div>
      <label className="block text-sm">Choose a PNG or JPEG of your signature on white paper
        <input type="file" accept="image/png,image/jpeg" className="mt-1 block"
          onChange={async (e) => {
            const f = e.target.files?.[0];
            if (!f) return;
            if (f.size > 20 * 1024 * 1024) { onError("That file is larger than 20 MB."); return; }
            setSrc(new Uint8Array(await f.arrayBuffer()));
          }} />
      </label>
      {src && (
        <>
          <label className="mt-3 flex items-center gap-2 text-sm">
            Background threshold
            <input type="range" min={20} max={254} value={threshold} onChange={(e) => setThreshold(Number(e.target.value))} aria-valuetext={`${threshold} of 255`} />
            <span className="w-8 tabular-nums">{threshold}</span>
          </label>
          <p className="text-xs text-neutral-500">Lighter pixels than the threshold become transparent. Lower it to remove more of the paper.</p>
          <div className="mt-2 grid h-40 place-items-center rounded border" style={{ backgroundImage: "repeating-conic-gradient(#ddd 0% 25%, #fff 0% 50%)", backgroundSize: "16px 16px" }}>
            {preview ? <img src={preview.url} alt="Processed signature preview" className="max-h-36 max-w-full" /> : <span className="text-sm">No preview</span>}
          </div>
          <div className="mt-2 flex justify-end">
            <button className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-40" disabled={!preview || saving || !name.trim()} onClick={() => preview && onSave(preview.bytes)}>Save signature</button>
          </div>
        </>
      )}
    </div>
  );
}
