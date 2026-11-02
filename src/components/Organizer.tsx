import { useEffect, useRef, useState } from "react";
import { PasswordDialog } from "./PasswordDialog";
import { useModal } from "../hooks/useModal";
import { useNear } from "../hooks/useNear";
import { toAppError } from "../lib/api";
import * as api from "../lib/organize-api";
import { dropGap, moveTarget, nudgeTarget } from "../lib/selection";
import { useOrganizer } from "../organizerStore";

const THUMB_W = 150;

function useOrgThumb(session: number, src: number, page: number, enabled: boolean, cssScale: number) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!enabled) {
      setUrl((o) => (o && URL.revokeObjectURL(o), null));
      return;
    }
    let alive = true;
    api
      .orgThumb(session, src, page, Math.round(cssScale * (window.devicePixelRatio || 1) * 100) / 100)
      .then((b) => alive && setUrl((o) => (o && URL.revokeObjectURL(o), URL.createObjectURL(b))))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [session, src, page, enabled, cssScale]);
  useEffect(() => () => setUrl((o) => (o && URL.revokeObjectURL(o), null)), []);
  return url;
}

function Cell({ index, dragging }: { index: number; dragging: boolean }) {
  const st = useOrganizer((s) => s.state)!;
  const selected = useOrganizer((s) => s.sel.selected.includes(index));
  const p = st.pages[index];
  const [ref, near] = useNear<HTMLDivElement>(600);
  const turned = p.rotate % 180 !== 0;
  const dw = turned ? p.height : p.width;
  const dh = turned ? p.width : p.height;
  const scale = THUMB_W / Math.max(dw, dh);
  const boxW = Math.round(dw * scale);
  const boxH = Math.round(dh * scale);
  // Render the unrotated page sized to its own width, then rotate in CSS.
  const imgScale = Math.round(p.width * scale);
  const url = useOrgThumb(st.session, p.src, p.page, near && p.kind === "source", scale);
  return (
    <div
      ref={ref}
      role="option"
      aria-selected={selected}
      aria-label={`Page ${index + 1}${p.kind === "blank" ? " (blank)" : ""}${p.rotate ? `, rotated ${p.rotate} degrees` : ""}`}
      tabIndex={-1}
      data-index={index}
      className={`flex select-none flex-col items-center rounded p-2 ${selected ? "bg-blue-100 ring-2 ring-blue-600 dark:bg-blue-950" : ""} ${dragging && selected ? "opacity-40" : ""}`}
      style={{ width: THUMB_W + 24 }}
    >
      <div className="relative flex items-center justify-center" style={{ width: THUMB_W, height: THUMB_W }}>
        <div className="relative overflow-hidden bg-white shadow" style={{ width: boxW, height: boxH }}>
          {p.kind === "blank" ? (
            <span className="absolute inset-0 grid place-items-center text-xs text-neutral-400">Blank</span>
          ) : (
            url && (
              <img
                src={url}
                alt=""
                draggable={false}
                className="absolute max-w-none"
                style={{
                  width: imgScale,
                  height: Math.round(p.height * scale),
                  left: "50%",
                  top: "50%",
                  transform: `translate(-50%, -50%) rotate(${p.rotate}deg)`,
                }}
              />
            )
          )}
        </div>
      </div>
      <div className="mt-1 text-xs">{index + 1}</div>
    </div>
  );
}

export function Organizer({ onClose }: { onClose: () => void }) {
  const o = useOrganizer();
  const st = o.state;
  const [blankSize, setBlankSize] = useState<"a4" | "letter" | "same">("same");
  const [splitOpen, setSplitOpen] = useState(false);
  const [drag, setDrag] = useState<{ gap: number; x: number; y: number } | null>(null);
  const grid = useRef<HTMLDivElement>(null);
  const focus = useRef(0);

  const sel = o.sel.selected;
  const count = st?.pages.length ?? 0;

  // ----- pointer reorder -----
  const down = useRef<{ x: number; y: number; index: number; started: boolean } | null>(null);
  const onPointerDown = (e: React.PointerEvent) => {
    const cell = (e.target as HTMLElement).closest<HTMLElement>("[data-index]");
    if (!cell || e.button !== 0) return;
    const index = Number(cell.dataset.index);
    focus.current = index;
    if (!(e.shiftKey || e.ctrlKey || e.metaKey) && !sel.includes(index)) o.select(index, { shift: false, ctrl: false });
    else if (e.shiftKey || e.ctrlKey || e.metaKey) o.select(index, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey });
    down.current = { x: e.clientX, y: e.clientY, index, started: false };
    grid.current?.setPointerCapture(e.pointerId);
    grid.current?.focus();
  };
  const gapAt = (x: number, y: number) => {
    const cells = Array.from(grid.current?.querySelectorAll<HTMLElement>("[data-index]") ?? []);
    return dropGap(cells.map((c) => c.getBoundingClientRect()), x, y);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = down.current;
    if (!d) return;
    if (!d.started && Math.hypot(e.clientX - d.x, e.clientY - d.y) > 6) d.started = true;
    if (d.started) setDrag({ gap: gapAt(e.clientX, e.clientY), x: e.clientX, y: e.clientY });
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const d = down.current;
    down.current = null;
    grid.current?.releasePointerCapture(e.pointerId);
    if (d?.started) {
      const to = moveTarget(useOrganizer.getState().sel.selected, gapAt(e.clientX, e.clientY));
      setDrag(null);
      if (to !== null) {
        const moved = useOrganizer.getState().sel.selected.length;
        void o.edit({ op: "move", indices: useOrganizer.getState().sel.selected, to }, () => ({
          selected: Array.from({ length: moved }, (_, i) => to + i),
          anchor: to,
        }));
      }
    } else if (d && !(e.shiftKey || e.ctrlKey || e.metaKey)) {
      o.select(d.index, { shift: false, ctrl: false }); // plain click on a selected page collapses to it
    }
  };

  // ----- actions -----
  const rotate = (deg: number) => sel.length && o.edit({ op: "rotate", indices: sel, degrees: deg });
  const del = () => sel.length && o.edit({ op: "delete", indices: sel }, () => ({ selected: [], anchor: null }));
  const dup = () => sel.length && o.edit({ op: "duplicate", indices: sel }, () => ({ selected: [], anchor: null }));
  const nudge = (dir: -1 | 1) => {
    const to = nudgeTarget(sel, dir, count);
    if (to !== null) void o.edit({ op: "move", indices: sel, to }, () => ({ selected: sel.map((_, i) => to + i), anchor: to }));
  };
  const addBlank = (where: "before" | "after") => {
    const at = sel.length === 0 ? count : where === "before" ? sel[0] : sel[sel.length - 1] + 1;
    const like = sel.length ? (where === "before" ? sel[0] : sel[sel.length - 1]) : Math.max(0, count - 1);
    void o.edit({ op: "insertBlank", at, size: blankSize, like }, () => ({ selected: [at], anchor: at }));
  };
  const insertFiles = async (append: boolean) => {
    const paths = await api.pickPdfs().catch((e) => (o.setError(toAppError(e)), []));
    let at = append || sel.length === 0 ? count : sel[sel.length - 1] + 1;
    for (const p of paths) {
      const before = useOrganizer.getState().state?.pages.length ?? 0;
      await o.addFile(p, undefined, append ? undefined : at);
      const after = useOrganizer.getState().state?.pages.length ?? 0;
      at += after - before;
    }
  };
  const report = (r: api.Saved) => {
    const lines = [`Saved ${r.preview.pageCount} pages (${api.formatBytes(r.preview.sizeBytes)}) to ${r.path}`];
    r.preview.renamedFields.forEach((f) => lines.push(`Form field renamed: "${f.from}" → "${f.to}" (from ${f.source})`));
    if (r.preview.droppedLinks) lines.push(`${r.preview.droppedLinks} link(s) to removed pages were dropped.`);
    if (r.preview.droppedBookmarks) lines.push(`${r.preview.droppedBookmarks} bookmark(s) to removed pages were dropped.`);
    r.preview.warnings.forEach((w) => lines.push(w));
    o.setNotice(lines);
  };
  const guarded = async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    o.setBusy(true);
    try {
      return await fn();
    } catch (e) {
      o.setError(toAppError(e));
    } finally {
      o.setBusy(false);
    }
  };
  const baseName = st?.sources[0]?.name.replace(/\.pdf$/i, "") ?? "document";
  const saveAs = () =>
    guarded(async () => {
      const r = await api.orgSaveAs(st!.session, `${baseName}${st!.sources.length > 1 ? "_merged" : "_edited"}.pdf`);
      if (r) report(r);
    });
  const extract = () =>
    sel.length > 0 &&
    guarded(async () => {
      const r = await api.orgSaveAs(st!.session, `${baseName}_extract.pdf`, sel);
      if (r) report(r);
    });
  const saveOver = () => {
    if (!window.confirm("Overwrite the original file? A backup copy (.bak) is kept next to it.")) return;
    void guarded(async () => report(await api.orgSaveOver(st!.session)));
  };

  // ----- keyboard -----
  const onKeyDown = (e: React.KeyboardEvent) => {
    const mod = e.ctrlKey || e.metaKey;
    const move = (to: number) => {
      const i = Math.max(0, Math.min(count - 1, to));
      focus.current = i;
      o.select(i, { shift: e.shiftKey, ctrl: false });
      grid.current?.querySelector<HTMLElement>(`[data-index="${i}"]`)?.scrollIntoView({ block: "nearest" });
    };
    if (mod && e.key.toLowerCase() === "a") { e.preventDefault(); o.selectAll(); }
    else if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); void o.edit({ op: e.shiftKey ? "redo" : "undo" }); }
    else if (mod && e.key.toLowerCase() === "y") { e.preventDefault(); void o.edit({ op: "redo" }); }
    else if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); void del(); }
    else if (e.altKey && e.key === "ArrowLeft") { e.preventDefault(); nudge(-1); }
    else if (e.altKey && e.key === "ArrowRight") { e.preventDefault(); nudge(1); }
    else if (e.key === "ArrowLeft") { e.preventDefault(); move(focus.current - 1); }
    else if (e.key === "ArrowRight") { e.preventDefault(); move(focus.current + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); move(focus.current - perRow()); }
    else if (e.key === "ArrowDown") { e.preventDefault(); move(focus.current + perRow()); }
    else if (e.key === " ") { e.preventDefault(); o.select(focus.current, { shift: false, ctrl: true }); }
    else if (e.key === "Escape") onClose();
  };
  const perRow = () => {
    const cells = Array.from(grid.current?.querySelectorAll<HTMLElement>("[data-index]") ?? []);
    if (cells.length < 2) return 1;
    const top = cells[0].getBoundingClientRect().top;
    const n = cells.findIndex((c) => Math.abs(c.getBoundingClientRect().top - top) > 4);
    return n < 0 ? cells.length : n;
  };

  if (!st) return null;
  const b = "rounded border px-2 py-1 text-sm disabled:opacity-40";
  const hasSel = sel.length > 0;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-wrap items-center gap-1.5 border-b px-3 py-2" role="toolbar" aria-label="Page organizer tools">
        <button className={b} onClick={onClose}>← Back to viewer</button>
        <span className="mx-1 h-5 border-l" aria-hidden />
        <button className={b} disabled={!st.canUndo} onClick={() => o.edit({ op: "undo" })} aria-keyshortcuts="Control+Z">Undo</button>
        <button className={b} disabled={!st.canRedo} onClick={() => o.edit({ op: "redo" })} aria-keyshortcuts="Control+Y">Redo</button>
        <span className="mx-1 h-5 border-l" aria-hidden />
        <button className={b} disabled={!hasSel} onClick={() => rotate(-90)}>Rotate ⟲</button>
        <button className={b} disabled={!hasSel} onClick={() => rotate(90)}>Rotate ⟳</button>
        <button className={b} disabled={!hasSel} onClick={dup}>Duplicate</button>
        <button className={b} disabled={!hasSel || sel.length >= count} onClick={del} aria-keyshortcuts="Delete">Delete</button>
        <button className={b} disabled={!hasSel} onClick={() => nudge(-1)} aria-keyshortcuts="Alt+ArrowLeft">Move earlier</button>
        <button className={b} disabled={!hasSel} onClick={() => nudge(1)} aria-keyshortcuts="Alt+ArrowRight">Move later</button>
        <span className="mx-1 h-5 border-l" aria-hidden />
        <label className="text-sm">
          <span className="sr-only">Blank page size</span>
          <select className="rounded border px-1 py-1 dark:bg-neutral-800" value={blankSize} onChange={(e) => setBlankSize(e.target.value as typeof blankSize)}>
            <option value="same">Blank: same size</option>
            <option value="a4">Blank: A4</option>
            <option value="letter">Blank: Letter</option>
          </select>
        </label>
        <button className={b} onClick={() => addBlank("before")}>Add blank before</button>
        <button className={b} onClick={() => addBlank("after")}>Add blank after</button>
        <span className="mx-1 h-5 border-l" aria-hidden />
        <button className={b} onClick={() => insertFiles(false)}>Insert PDF here…</button>
        <button className={b} onClick={() => insertFiles(true)}>Add PDFs (merge)…</button>
        <button className={b} disabled={!hasSel} onClick={extract}>Extract selected…</button>
        <button className={b} onClick={() => setSplitOpen(true)}>Split…</button>
        <span className="mx-1 h-5 border-l" aria-hidden />
        <button className={b} onClick={() => o.refreshPreview()} disabled={o.busy}>Preview output</button>
        <button className="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-40" disabled={o.busy} onClick={saveAs}>Save a copy…</button>
        <button className={b} disabled={!st.canSaveOver || o.busy} onClick={saveOver}>Save (overwrite, keeps .bak)</button>
      </div>

      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 border-b bg-neutral-100 px-3 py-1 text-xs dark:bg-neutral-800" aria-live="polite">
        <span>{count} pages · {sel.length} selected</span>
        <span>Files: {st.sources.map((s) => `${s.name} (${s.pageCount})`).join(", ") || "none"}</span>
        {o.preview && (
          <span>
            Output: {o.preview.pageCount} pages, {api.formatBytes(o.preview.sizeBytes)}
            {o.preview.renamedFields.length > 0 && ` · ${o.preview.renamedFields.length} form field(s) will be renamed`}
          </span>
        )}
        {o.busy && <span>Working…</span>}
      </div>

      {o.error && (
        <div role="alert" className="flex justify-between border-b border-amber-300 bg-amber-100 px-3 py-2 text-sm text-amber-900">
          <span>{o.error.message}</span>
          <button className="underline" onClick={() => o.setError(null)}>Dismiss</button>
        </div>
      )}
      {o.preview && o.preview.renamedFields.length > 0 && (
        <div role="status" className="border-b border-sky-300 bg-sky-50 px-3 py-2 text-sm text-sky-900">
          <strong>Form fields that will be renamed to avoid collisions:</strong>
          <ul className="list-inside list-disc">
            {o.preview.renamedFields.map((f, i) => <li key={i}>"{f.from}" → "{f.to}" ({f.source})</li>)}
          </ul>
        </div>
      )}
      {o.notice && (
        <div role="status" className="border-b border-green-300 bg-green-50 px-3 py-2 text-sm text-green-900">
          {o.notice.map((l, i) => <div key={i}>{l}</div>)}
          <button className="mt-1 underline" onClick={() => o.setNotice(null)}>Dismiss</button>
        </div>
      )}

      <div
        ref={grid}
        role="listbox"
        aria-multiselectable="true"
        aria-label="Pages. Arrow keys move, Space toggles, Alt+Arrow reorders."
        tabIndex={0}
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        className="relative flex flex-1 flex-wrap content-start gap-2 overflow-auto bg-neutral-200 p-4 outline-none focus-visible:ring-2 dark:bg-neutral-700"
        style={{ touchAction: "none" }}
      >
        {st.pages.map((_, i) => (
          <div key={i} className="relative">
            {drag && drag.gap === i && <div className="absolute -left-1.5 top-0 z-10 h-full w-1 rounded bg-blue-600" aria-hidden />}
            <Cell index={i} dragging={!!drag} />
            {drag && drag.gap === count && i === count - 1 && <div className="absolute -right-1.5 top-0 z-10 h-full w-1 rounded bg-blue-600" aria-hidden />}
          </div>
        ))}
        {drag && (
          <div className="pointer-events-none fixed z-20 rounded bg-blue-600 px-2 py-1 text-xs text-white" style={{ left: drag.x + 12, top: drag.y + 12 }}>
            {sel.length} page{sel.length === 1 ? "" : "s"}
          </div>
        )}
      </div>

      {o.pendingPassword && (
        <PasswordDialog
          fileName={o.pendingPassword.path.split(/[\\/]/).pop() ?? ""}
          wrong={o.pendingPassword.wrong}
          onSubmit={(pw) => o.addFile(o.pendingPassword!.path, pw, o.pendingPassword!.insertAt)}
          onCancel={() => o.setPendingPassword(null)}
        />
      )}
      {splitOpen && <SplitDialog session={st.session} onClose={() => setSplitOpen(false)} />}
    </div>
  );
}

function SplitDialog({ session, onClose }: { session: number; onClose: () => void }) {
  const o = useOrganizer();
  const [mode, setMode] = useState<"everyN" | "ranges" | "bookmarks">("everyN");
  const [n, setN] = useState(1);
  const [spec, setSpec] = useState("");
  const [busy, setBusy] = useState(false);
  const dlg = useModal<HTMLFormElement>(onClose);
  const run = async () => {
    setBusy(true);
    try {
      const m: api.SplitMode = mode === "everyN" ? { mode, n } : mode === "ranges" ? { mode, spec } : { mode };
      const r = await api.orgSplit(session, m);
      if (r) o.setNotice([`Wrote ${r.files.length} file(s) to ${r.folder}`, ...r.files.slice(0, 20)]);
      onClose();
    } catch (e) {
      o.setError(toAppError(e));
      onClose();
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="fixed inset-0 z-10 grid place-items-center bg-black/40">
      <form
        ref={dlg}
        role="dialog" aria-modal="true" aria-labelledby="split-h"
        className="w-[28rem] rounded-lg bg-white p-5 shadow-xl dark:bg-neutral-800"
        onSubmit={(e) => { e.preventDefault(); void run(); }}
      >
        <h2 id="split-h" className="mb-3 text-lg font-semibold">Split document</h2>
        <fieldset className="space-y-2 text-sm">
          <legend className="sr-only">Split mode</legend>
          <label className="flex items-center gap-2"><input type="radio" checked={mode === "everyN"} onChange={() => setMode("everyN")} />Every
            <input type="number" min={1} value={n} onChange={(e) => setN(Math.max(1, Number(e.target.value)))} className="w-16 rounded border px-1 dark:bg-neutral-900" aria-label="Pages per file" /> page(s)</label>
          <label className="flex items-center gap-2"><input type="radio" checked={mode === "ranges"} onChange={() => setMode("ranges")} />Page ranges
            <input value={spec} onChange={(e) => setSpec(e.target.value)} placeholder="1-3, 4-6, 7-" className="flex-1 rounded border px-1 dark:bg-neutral-900" aria-label="Page ranges" /></label>
          <label className="flex items-center gap-2"><input type="radio" checked={mode === "bookmarks"} onChange={() => setMode("bookmarks")} />At top-level bookmarks</label>
        </fieldset>
        <p className="mt-3 text-xs text-neutral-500">You will choose an output folder. Existing files are never overwritten.</p>
        <div className="mt-4 flex justify-end gap-2">
          <button type="button" className="rounded border px-3 py-1" onClick={onClose}>Cancel</button>
          <button type="submit" disabled={busy} className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-40">Split…</button>
        </div>
      </form>
    </div>
  );
}
