import type { Layout } from "../lib/layout";
import { clampPercent, stepZoom } from "../lib/zoom";
import { useOrganizer } from "../organizerStore";
import { useViewer } from "../store";

interface Props {
  onOpen: () => void;
  onPrint: () => void;
  onOrganize: () => void;
  onMerge: () => void;
}

const LAYOUTS: { id: Layout; label: string }[] = [
  { id: "continuous", label: "Continuous" },
  { id: "single", label: "Single page" },
  { id: "two-page", label: "Two pages" },
];

export function Toolbar({ onOpen, onPrint, onOrganize, onMerge }: Props) {
  const v = useViewer();
  const orgActive = useOrganizer((s) => s.active);
  const doc = orgActive ? null : v.doc;
  const percent = v.zoom.kind === "percent" ? v.zoom.value : Math.round(v.effectiveScale * 100);
  const btn = "rounded border px-2 py-1 disabled:opacity-40";
  return (
    <header className="flex flex-wrap items-center gap-2 border-b px-3 py-2" role="toolbar" aria-label="Viewer controls">
      <button className="rounded bg-blue-600 px-3 py-1 text-white" onClick={onOpen} disabled={v.busy} aria-keyshortcuts="Control+O">
        Open…
      </button>
      <button className={btn} onClick={onMerge} disabled={v.busy}>Merge PDFs…</button>
      {doc && (
        <>
          <button className={btn} onClick={onOrganize}>Organize pages</button>
          <button className={btn} aria-pressed={v.sidebar !== null} onClick={() => v.setSidebar(v.sidebar ? null : "thumbnails")} aria-keyshortcuts="F4">
            Sidebar
          </button>
          <span className="max-w-56 truncate text-sm" title={doc.fileName}>{doc.fileName}</span>
          <button aria-label="Previous page" className={btn} onClick={() => v.goToPage(v.page - 1)} disabled={v.page === 0}>◀</button>
          <label className="text-sm">
            <span className="sr-only">Page number</span>
            <input
              type="number" min={1} max={doc.pageCount} value={v.page + 1}
              onChange={(e) => v.goToPage(Number(e.target.value) - 1)}
              className="w-16 rounded border px-1 py-0.5 dark:bg-neutral-800"
            />
          </label>
          <span className="text-sm">/ {doc.pageCount}</span>
          <button aria-label="Next page" className={btn} onClick={() => v.goToPage(v.page + 1)} disabled={v.page >= doc.pageCount - 1}>▶</button>
          <span className="mx-1 h-5 border-l" aria-hidden />
          <button aria-label="Zoom out" className={btn} onClick={() => v.setZoom({ kind: "percent", value: stepZoom(percent, -1) })} aria-keyshortcuts="Control+Minus">−</button>
          <span className="w-14 text-center text-sm" aria-live="polite">{percent}%</span>
          <button aria-label="Zoom in" className={btn} onClick={() => v.setZoom({ kind: "percent", value: stepZoom(percent, 1) })} aria-keyshortcuts="Control+Plus">+</button>
          <button className={btn} aria-pressed={v.zoom.kind === "fit-width"} onClick={() => v.setZoom({ kind: "fit-width" })}>Fit width</button>
          <button className={btn} aria-pressed={v.zoom.kind === "fit-page"} onClick={() => v.setZoom({ kind: "fit-page" })}>Fit page</button>
          <label className="text-sm">
            <span className="sr-only">Zoom percent</span>
            <input
              type="number" min={25} max={400} step={5} value={percent}
              onChange={(e) => v.setZoom({ kind: "percent", value: clampPercent(Number(e.target.value)) })}
              className="w-16 rounded border px-1 py-0.5 dark:bg-neutral-800"
            />
          </label>
          <label className="text-sm">
            <span className="sr-only">Page layout</span>
            <select value={v.layout} onChange={(e) => v.setLayout(e.target.value as Layout)} className="rounded border px-1 py-1 dark:bg-neutral-800">
              {LAYOUTS.map((l) => <option key={l.id} value={l.id}>{l.label}</option>)}
            </select>
          </label>
          <button className={btn} onClick={() => v.setSidebar("search")} aria-keyshortcuts="Control+F">Find</button>
          <button className={btn} onClick={onPrint} aria-keyshortcuts="Control+P">Print…</button>
        </>
      )}
    </header>
  );
}
