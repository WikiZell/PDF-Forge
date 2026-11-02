import { useEffect, useRef, useState } from "react";
import { PasswordDialog } from "./PasswordDialog";
import { SigPreview } from "./SigPreview";
import { SignatureDialog } from "./SignatureDialog";
import { toAppError } from "../lib/api";
import { forgetSigImage } from "../hooks/useSigImage";
import { measureText } from "../lib/measure";
import { newId, textBox } from "../lib/placements";
import { signDocument } from "../lib/sig-api";
import { useSign } from "../signStore";
import { useViewer } from "../store";

export function SignPanel() {
  const s = useSign();
  const doc = useViewer((v) => v.doc);
  const [dialog, setDialog] = useState(false);
  const [needPw, setNeedPw] = useState<{ wrong: boolean } | null>(null);
  const placements = s.history.present;
  const selected = placements.find((p) => p.id === s.selectedId);
  const before = useRef(placements);

  useEffect(() => { void s.loadLibrary(); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const save = async (password?: string) => {
    if (!doc) return;
    s.setBusy(true);
    s.setError(null);
    try {
      const r = await signDocument(doc.path, placements, s.flatten, password);
      setNeedPw(null);
      if (r) s.setNotice([`Saved a signed copy with ${r.applied} item(s) to ${r.path}`, ...r.warnings]);
    } catch (e) {
      const err = toAppError(e);
      if (err.code === "password_required" || err.code === "wrong_password") setNeedPw({ wrong: err.code === "wrong_password" });
      else s.setError(err);
    } finally {
      s.setBusy(false);
    }
  };

  const updateText = (patch: { text?: string; fontSize?: number }) => {
    if (!selected || selected.kind !== "text") return;
    const text = patch.text ?? selected.text ?? "";
    const fontSize = patch.fontSize ?? selected.fontSize ?? 12;
    const b = textBox(text, fontSize, measureText);
    s.liveUpdate(placements.map((p) => (p.id === selected.id ? { ...p, text, fontSize, w: b.w, h: b.h } : p)));
  };

  // drag a library item onto a page (click-to-place is the keyboard/primary path)
  const dragFromLibrary = (e: React.PointerEvent, sigId: string) => {
    const startX = e.clientX, startY = e.clientY;
    let moved = false;
    const onMove = (ev: PointerEvent) => { if (Math.hypot(ev.clientX - startX, ev.clientY - startY) > 8) moved = true; };
    const onUp = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      if (!moved) return;
      const el = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-page]");
      if (!el) return;
      const page = Number(el.dataset.page);
      const rect = el.getBoundingClientRect();
      const sizes = useViewer.getState().sizes;
      const scale = rect.width / (sizes[page]?.width || rect.width);
      const sig = useSign.getState().library.find((x) => x.id === sigId);
      if (!sig || !sizes[page]) return;
      const w = Math.min(220, Math.max(70, sizes[page].width * 0.3));
      const h = w / (sig.aspect || 3);
      const x = (ev.clientX - rect.left) / scale - w / 2, y = (ev.clientY - rect.top) / scale - h / 2;
      const st = useSign.getState();
      const id = newId();
      st.commitPlacements([...st.history.present, { id, page, x, y, w, h, rotation: 0, kind: "signature", sigId }]);
      st.select(id);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  const b = "rounded border px-2 py-1 text-sm disabled:opacity-40";
  return (
    <aside className="flex w-72 shrink-0 flex-col overflow-auto border-l p-3 text-sm" aria-label="Fill and sign">
      <h2 className="mb-2 text-base font-semibold">Fill &amp; Sign</h2>

      <section aria-labelledby="sigs-h">
        <div className="mb-1 flex items-center justify-between">
          <h3 id="sigs-h" className="font-semibold">Signatures</h3>
          <button className={b} onClick={() => setDialog(true)}>New…</button>
        </div>
        {s.library.length === 0 && <p className="text-neutral-500">No saved signatures yet. Draw, type or import one.</p>}
        <ul className="space-y-2">
          {s.library.map((sig) => (
            <li key={sig.id} className={`rounded border p-1.5 ${s.armed?.kind === "signature" && s.armed.sigId === sig.id ? "ring-2 ring-blue-600" : ""}`}>
              <button
                className="block h-14 w-full cursor-grab rounded bg-white px-2 py-1"
                aria-label={`Place signature ${sig.name}`}
                title="Click, then click on a page — or drag onto a page"
                onClick={() => s.setArmed(s.armed?.kind === "signature" && s.armed.sigId === sig.id ? null : { kind: "signature", sigId: sig.id })}
                onPointerDown={(e) => dragFromLibrary(e, sig.id)}
              >
                <SigPreview sig={sig} fit className="mx-auto h-full w-full" />
              </button>
              <div className="mt-1 flex items-center gap-1">
                <span className="min-w-0 flex-1 truncate" title={sig.name}>{sig.name}</span>
                <button className="px-1 underline" onClick={() => { const n = window.prompt("Rename signature", sig.name); if (n) void s.rename(sig.id, n); }}>Rename</button>
                <button className="px-1 text-red-700 underline" onClick={() => { if (window.confirm(`Delete "${sig.name}"?`)) { forgetSigImage(sig.id); void s.remove(sig.id); } }}>Delete</button>
              </div>
            </li>
          ))}
        </ul>
      </section>

      <section className="mt-4" aria-labelledby="tools-h">
        <h3 id="tools-h" className="mb-1 font-semibold">Fields</h3>
        <div className="flex gap-2">
          <button className={b} aria-pressed={s.armed?.kind === "date"} onClick={() => s.setArmed(s.armed?.kind === "date" ? null : { kind: "date" })}>Date</button>
          <button className={b} aria-pressed={s.armed?.kind === "text"} onClick={() => s.setArmed(s.armed?.kind === "text" ? null : { kind: "text" })}>Text</button>
        </div>
        {s.armed && <p className="mt-1 text-xs text-neutral-500" role="status">Click on the page to place. Esc cancels.</p>}
      </section>

      {selected?.kind === "text" && (
        <section className="mt-4" aria-labelledby="props-h">
          <h3 id="props-h" className="mb-1 font-semibold">Selected text</h3>
          <label className="block">Text
            <input className="mt-1 w-full rounded border px-2 py-1 dark:bg-neutral-900" value={selected.text ?? ""}
              onFocus={() => { before.current = placements; }}
              onChange={(e) => updateText({ text: e.target.value })}
              onBlur={() => s.endDrag(before.current)} />
          </label>
          <label className="mt-1 block">Size
            <input type="number" min={4} max={200} className="ml-2 w-16 rounded border px-1 dark:bg-neutral-900" value={selected.fontSize ?? 12}
              onFocus={() => { before.current = placements; }}
              onChange={(e) => updateText({ fontSize: Math.min(200, Math.max(4, Number(e.target.value) || 12)) })}
              onBlur={() => s.endDrag(before.current)} />
          </label>
        </section>
      )}

      <section className="mt-4">
        <div className="flex flex-wrap gap-2">
          <button className={b} disabled={s.history.past.length === 0} onClick={s.undo} aria-keyshortcuts="Control+Z">Undo</button>
          <button className={b} disabled={s.history.future.length === 0} onClick={s.redo} aria-keyshortcuts="Control+Y">Redo</button>
          <button className={b} disabled={!s.selectedId} onClick={s.deleteSelected} aria-keyshortcuts="Delete">Delete</button>
        </div>
        <p className="mt-1 text-xs text-neutral-500">{placements.length} item(s) placed. Arrow keys nudge the selection; Shift+resize frees the aspect ratio.</p>
      </section>

      <section className="mt-4">
        <label className="flex items-start gap-2">
          <input type="checkbox" className="mt-1" checked={s.flatten} onChange={(e) => s.setFlatten(e.target.checked)} />
          <span>Flatten on save<br /><span className="text-xs text-neutral-500">Bakes everything into the page so it can no longer be moved or removed. Off = editable annotations.</span></span>
        </label>
        <button className="mt-3 w-full rounded bg-blue-600 px-3 py-2 text-white disabled:opacity-40" disabled={placements.length === 0 || s.busy} onClick={() => save()}>
          Save signed copy…
        </button>
        <p className="mt-1 text-xs text-neutral-500">The original file is never modified.</p>
      </section>

      {s.error && <p role="alert" className="mt-3 rounded bg-amber-100 p-2 text-amber-900">{s.error.message}</p>}
      {s.notice && (
        <div role="status" className="mt-3 rounded bg-green-50 p-2 text-green-900">
          {s.notice.map((n, i) => <div key={i}>{n}</div>)}
        </div>
      )}

      {dialog && <SignatureDialog onClose={() => setDialog(false)} />}
      {needPw && doc && (
        <PasswordDialog fileName={doc.fileName} wrong={needPw.wrong} onSubmit={(pw) => save(pw)} onCancel={() => setNeedPw(null)} />
      )}
    </aside>
  );
}
