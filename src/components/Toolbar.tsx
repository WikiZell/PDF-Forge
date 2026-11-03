import type { Layout } from "../lib/layout";
import { clampPercent, stepZoom } from "../lib/zoom";
import { useOrganizer } from "../organizerStore";
import { useSign } from "../signStore";
import { useViewer } from "../store";
import { Icon } from "./Icon";

interface Props {
  onOpen: () => void;
  onPrint: () => void;
  onOrganize: () => void;
  onMerge: () => void;
  onBulk: () => void;
  onSettings: () => void;
}

const LAYOUTS: { id: Layout; label: string }[] = [
  { id: "continuous", label: "Continuous" },
  { id: "single", label: "Single page" },
  { id: "two-page", label: "Two pages" },
];

export function Toolbar({ onOpen, onPrint, onOrganize, onMerge, onBulk, onSettings }: Props) {
  const v = useViewer();
  const orgActive = useOrganizer((s) => s.active);
  const signing = useSign((s) => s.active);
  const doc = orgActive ? null : v.doc;
  const percent = v.zoom.kind === "percent" ? v.zoom.value : Math.round(v.effectiveScale * 100);
  // Labels collapse to icon-only on narrower windows; they stay in the accessibility tree (sr-only).
  const lbl = "max-xl:sr-only";
  return (
    <header className="shrink-0 border-b" style={{ background: "var(--surface)", borderColor: "var(--border)" }} role="toolbar" aria-label="Viewer controls">
      {/* Tier 1: app actions, modes, document identity */}
      <div className="flex items-center gap-2 px-3 py-2">
        <div className="mr-1 flex items-center gap-2 pr-1 select-none" aria-hidden>
          <span className="grid h-7 w-7 place-items-center rounded-lg text-white" style={{ background: "linear-gradient(135deg, var(--accent), #7c3aed)" }}>
            <Icon name="pdf" size={16} />
          </span>
        </div>
        <button className="btn btn-primary" onClick={onOpen} disabled={v.busy} aria-keyshortcuts="Control+O">
          <Icon name="open" /><span>Open…</span>
        </button>
        <div className="group" role="group" aria-label="Tools">
          <button className="btn" onClick={onMerge} disabled={v.busy}><Icon name="merge" /><span className={lbl}>Merge PDFs…</span></button>
          <button className="btn" onClick={onBulk}><Icon name="bulk" /><span className={lbl}>Bulk sign…</span></button>
          {doc && (
            <>
              <button className="btn" onClick={onOrganize}><Icon name="organize" /><span className={lbl}>Organize pages</span></button>
              <button className="btn" aria-pressed={signing} onClick={() => useSign.getState().setActive(!signing)}>
                <Icon name="sign" /><span className={lbl}>Fill &amp; Sign</span>
              </button>
            </>
          )}
        </div>
        <div className="min-w-0 flex-1 text-center">
          {doc && (
            <span className="inline-flex max-w-full items-center gap-2 rounded-full px-3 py-1 text-[13px] font-medium" style={{ background: "var(--surface-2)", border: "1px solid var(--border)" }} title={doc.fileName}>
              <span className="truncate">{doc.fileName}</span>
            </span>
          )}
        </div>
        {doc && (
          <>
            <button className="btn btn-quiet" onClick={() => v.setSidebar("search")} aria-keyshortcuts="Control+F"><Icon name="find" /><span className={lbl}>Find</span></button>
            <button className="btn btn-quiet" onClick={onPrint} aria-keyshortcuts="Control+P"><Icon name="print" /><span className={lbl}>Print…</span></button>
          </>
        )}
        <button className="btn btn-quiet" onClick={onSettings} aria-label="Settings"><Icon name="settings" /><span className={lbl}>Settings</span></button>
      </div>

      {/* Tier 2: view controls */}
      {doc && (
        <div className="flex flex-wrap items-center gap-2 border-t px-3 py-1.5" style={{ borderColor: "var(--border)", background: "var(--bg)" }}>
          <button className="btn btn-sm" aria-pressed={v.sidebar !== null} onClick={() => v.setSidebar(v.sidebar ? null : "thumbnails")} aria-keyshortcuts="F4">
            <Icon name="sidebar" size={16} /><span>Sidebar</span>
          </button>
          <span className="divider" aria-hidden />
          <div className="group" role="group" aria-label="Page navigation">
            <button aria-label="Previous page" className="btn btn-icon" onClick={() => v.goToPage(v.page - 1)} disabled={v.page === 0}><Icon name="prev" /></button>
            <label>
              <span className="sr-only">Page number</span>
              <input type="number" min={1} max={doc.pageCount} value={v.page + 1}
                onChange={(e) => v.goToPage(Number(e.target.value) - 1)} className="input input-num" />
            </label>
            <span className="px-1 text-[13px]" style={{ color: "var(--muted)" }}>/ {doc.pageCount}</span>
            <button aria-label="Next page" className="btn btn-icon" onClick={() => v.goToPage(v.page + 1)} disabled={v.page >= doc.pageCount - 1}><Icon name="next" /></button>
          </div>
          <span className="divider" aria-hidden />
          <div className="group" role="group" aria-label="Zoom">
            <button aria-label="Zoom out" className="btn btn-icon" onClick={() => v.setZoom({ kind: "percent", value: stepZoom(percent, -1) })} aria-keyshortcuts="Control+Minus"><Icon name="minus" /></button>
            <span className="w-12 text-center text-[13px] font-medium tabular-nums" aria-live="polite">{percent}%</span>
            <button aria-label="Zoom in" className="btn btn-icon" onClick={() => v.setZoom({ kind: "percent", value: stepZoom(percent, 1) })} aria-keyshortcuts="Control+Plus"><Icon name="plus" /></button>
            <label>
              <span className="sr-only">Zoom percent</span>
              <input type="number" min={25} max={400} step={1} value={percent}
                onChange={(e) => v.setZoom({ kind: "percent", value: clampPercent(Number(e.target.value)) })} className="input input-num" />
            </label>
          </div>
          <div className="group" role="group" aria-label="Fit">
            <button className="btn btn-sm" aria-pressed={v.zoom.kind === "fit-width"} onClick={() => v.setZoom({ kind: "fit-width" })}><Icon name="fitw" size={16} /><span>Fit width</span></button>
            <button className="btn btn-sm" aria-pressed={v.zoom.kind === "fit-page"} onClick={() => v.setZoom({ kind: "fit-page" })}><Icon name="fitp" size={16} /><span>Fit page</span></button>
          </div>
          <label>
            <span className="sr-only">Page layout</span>
            <select value={v.layout} onChange={(e) => v.setLayout(e.target.value as Layout)} className="input">
              {LAYOUTS.map((l) => <option key={l.id} value={l.id}>{l.label}</option>)}
            </select>
          </label>
        </div>
      )}
    </header>
  );
}
