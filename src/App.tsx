import { useCallback, useEffect, useState } from "react";
import { PageView } from "./components/PageView";
import { PasswordDialog } from "./components/PasswordDialog";
import {
  closePdf, onOpenRequest, openPdf, pdfiumStatus, pickPdf, takeLaunchPath, toAppError,
} from "./lib/api";
import { clampPercent, stepZoom } from "./lib/zoom";
import { useViewer } from "./store";

const basename = (p: string) => p.split(/[\\/]/).pop() ?? p;

export function App() {
  const v = useViewer();
  const [libError, setLibError] = useState<string | null>(null);
  const [wrongPw, setWrongPw] = useState(false);

  const load = useCallback(async (path: string, password?: string) => {
    v.setBusy(true);
    try {
      const prev = useViewer.getState().doc;
      const doc = await openPdf(path, password);
      if (prev) closePdf(prev.id).catch(() => {});
      v.setPendingPassword(null);
      setWrongPw(false);
      v.setDoc(doc);
    } catch (e) {
      const err = toAppError(e);
      if (err.code === "password_required" || err.code === "wrong_password") {
        setWrongPw(err.code === "wrong_password");
        v.setPendingPassword(path);
      } else {
        v.setPendingPassword(null);
        v.setError(err);
      }
    } finally {
      v.setBusy(false);
    }
  }, [v]);

  const openDialog = useCallback(async () => {
    const path = await pickPdf().catch((e) => (v.setError(toAppError(e)), null));
    if (path) await load(path);
  }, [load, v]);

  useEffect(() => {
    pdfiumStatus().then((s) => !s.ok && setLibError(s.message)).catch(() => {});
    takeLaunchPath().then((p) => { if (p) void load(p); }).catch(() => {});
    const un = onOpenRequest((p) => load(p));
    return () => { un.then((f) => f()); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = useViewer.getState();
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") { e.preventDefault(); openDialog(); }
      else if (e.target instanceof HTMLInputElement) return;
      else if (e.key === "PageDown" || e.key === "ArrowRight") s.goToPage(s.page + 1);
      else if (e.key === "PageUp" || e.key === "ArrowLeft") s.goToPage(s.page - 1);
      else if (e.key === "Home") s.goToPage(0);
      else if (e.key === "End" && s.doc) s.goToPage(s.doc.pageCount - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openDialog]);

  const percent = v.zoom.kind === "percent" ? v.zoom.value : 100;

  return (
    <div className="flex h-screen flex-col bg-neutral-50 text-neutral-900 dark:bg-neutral-900 dark:text-neutral-100">
      <header className="flex flex-wrap items-center gap-2 border-b px-3 py-2" role="toolbar" aria-label="Viewer controls">
        <button className="rounded bg-blue-600 px-3 py-1 text-white" onClick={openDialog} disabled={v.busy}>
          Open… <span className="sr-only">(Ctrl+O)</span>
        </button>
        {v.doc && (
          <>
            <span className="max-w-64 truncate text-sm" title={v.doc.fileName}>{v.doc.fileName}</span>
            <button aria-label="Previous page" className="rounded border px-2 py-1" onClick={() => v.goToPage(v.page - 1)} disabled={v.page === 0}>◀</button>
            <label className="text-sm">
              <span className="sr-only">Page number</span>
              <input
                type="number" min={1} max={v.doc.pageCount} value={v.page + 1}
                onChange={(e) => v.goToPage(Number(e.target.value) - 1)}
                className="w-16 rounded border px-1 py-0.5 dark:bg-neutral-800"
              />
            </label>
            <span className="text-sm">/ {v.doc.pageCount}</span>
            <button aria-label="Next page" className="rounded border px-2 py-1" onClick={() => v.goToPage(v.page + 1)} disabled={v.page >= v.doc.pageCount - 1}>▶</button>
            <span className="mx-2 h-5 border-l" aria-hidden />
            <button aria-label="Zoom out" className="rounded border px-2 py-1" onClick={() => v.setZoom({ kind: "percent", value: stepZoom(percent, -1) })}>−</button>
            <span className="w-12 text-center text-sm" aria-live="polite">{v.zoom.kind === "percent" ? `${percent}%` : v.zoom.kind === "fit-width" ? "Fit W" : "Fit P"}</span>
            <button aria-label="Zoom in" className="rounded border px-2 py-1" onClick={() => v.setZoom({ kind: "percent", value: stepZoom(percent, 1) })}>+</button>
            <button className="rounded border px-2 py-1" onClick={() => v.setZoom({ kind: "fit-width" })}>Fit width</button>
            <button className="rounded border px-2 py-1" onClick={() => v.setZoom({ kind: "fit-page" })}>Fit page</button>
            <label className="text-sm">
              <span className="sr-only">Zoom percent</span>
              <input
                type="number" min={25} max={400} step={5} value={percent}
                onChange={(e) => v.setZoom({ kind: "percent", value: clampPercent(Number(e.target.value)) })}
                className="w-16 rounded border px-1 py-0.5 dark:bg-neutral-800"
              />
            </label>
          </>
        )}
      </header>

      {libError && (
        <div role="alert" className="border-b border-red-300 bg-red-100 px-3 py-2 text-sm text-red-900">
          {libError}
        </div>
      )}
      {v.error && (
        <div role="alert" className="flex items-center justify-between border-b border-amber-300 bg-amber-100 px-3 py-2 text-sm text-amber-900">
          <span>{v.error.message}</span>
          <button className="underline" onClick={() => v.setError(null)}>Dismiss</button>
        </div>
      )}

      {v.doc ? (
        <PageView />
      ) : (
        <main className="grid flex-1 place-items-center text-center">
          <div>
            <h1 className="mb-2 text-2xl font-semibold">PDF Forge</h1>
            <p className="text-neutral-600 dark:text-neutral-300">Open a PDF with Ctrl+O or drop a file on this window.</p>
          </div>
        </main>
      )}

      {v.pendingPasswordPath && (
        <PasswordDialog
          fileName={basename(v.pendingPasswordPath)}
          wrong={wrongPw}
          onSubmit={(pw) => load(v.pendingPasswordPath!, pw)}
          onCancel={() => v.setPendingPassword(null)}
        />
      )}
    </div>
  );
}
