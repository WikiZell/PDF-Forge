import { useCallback, useEffect, useState } from "react";
import { PasswordDialog } from "./components/PasswordDialog";
import { BulkSign } from "./components/BulkSign";
import { Organizer } from "./components/Organizer";
import { pickPdfs } from "./lib/organize-api";
import { useOrganizer } from "./organizerStore";
import { useMediaQuery } from "./hooks/useMediaQuery";
import { SettingsDialog } from "./components/SettingsDialog";
import { SignPanel } from "./components/SignPanel";
import { useSign } from "./signStore";
import { Sidebar } from "./components/Sidebar";
import { Toolbar } from "./components/Toolbar";
import { Viewer } from "./components/Viewer";
import { Welcome } from "./components/Welcome";
import {
  closePdf, onOpenRequest, openPdf, openRecent, pageSizes, pdfiumStatus, pickPdf,
  saveViewState, takeLaunchPath, toAppError,
} from "./lib/api";
import { printDocument } from "./lib/print";
import { stepZoom, serializeZoom } from "./lib/zoom";
import { useViewer } from "./store";

const basename = (p: string) => p.split(/[\\/]/).pop() ?? p;

export function App() {
  const v = useViewer();
  const org = useOrganizer();
  const sign = useSign();
  const [bulkOpen, setBulkOpen] = useState(false);
  // On narrow windows (small screens or 150-200 % display scaling) two side panels would squeeze the page
  // to a sliver, so the thumbnail sidebar steps aside while the Fill & Sign panel is open.
  const narrow = useMediaQuery("(max-width: 900px)");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [recentKey, setRecentKey] = useState(0);
  const [libError, setLibError] = useState<string | null>(null);
  const [wrongPw, setWrongPw] = useState(false);

  const load = useCallback(async (path: string, password?: string) => {
    const s = useViewer.getState();
    s.setBusy(true);
    try {
      const prev = s.doc;
      const doc = await openPdf(path, password);
      const sizes = await pageSizes(doc.id);
      if (prev) closePdf(prev.id).catch(() => {});
      s.setPendingPassword(null);
      setWrongPw(false);
      // placements belong to the document they were made on; never carry them over to another file
      if (prev?.path !== doc.path) useSign.getState().reset();
      s.setDoc(doc);
      s.setSizes(sizes);
    } catch (e) {
      const err = toAppError(e);
      if (err.code === "password_required" || err.code === "wrong_password") {
        setWrongPw(err.code === "wrong_password");
        s.setPendingPassword(path);
      } else {
        s.setPendingPassword(null);
        s.setError(err);
      }
    } finally {
      s.setBusy(false);
    }
  }, []);

  const openDialog = useCallback(async () => {
    const path = await pickPdf().catch((e) => (useViewer.getState().setError(toAppError(e)), null));
    if (path) await load(path);
  }, [load]);

  const openFromRecent = useCallback(async (p: string) => {
    try {
      await load(await openRecent(p));
    } catch (e) {
      useViewer.getState().setError(toAppError(e));
    }
  }, [load]);

  const print = useCallback(async () => {
    const s = useViewer.getState();
    if (!s.doc) return;
    try {
      await printDocument(s.doc.id, s.sizes);
    } catch (e) {
      s.setError({ code: "internal", message: e instanceof Error ? e.message : String(e) });
    }
  }, []);

  useEffect(() => {
    pdfiumStatus().then((s) => !s.ok && setLibError(s.message)).catch(() => {});
    takeLaunchPath().then((p) => { if (p) void load(p); }).catch(() => {});
    const un = onOpenRequest((p) => load(p));
    return () => { un.then((f) => f()); };
  }, [load]);

  // Persist the view position per file (debounced). Only a page number and zoom — no content.
  useEffect(() => {
    if (!v.doc) return;
    const t = setTimeout(() => {
      saveViewState(v.doc!.path, { page: v.page, zoom: serializeZoom(v.zoom), layout: v.layout }).catch(() => {});
    }, 600);
    return () => clearTimeout(t);
  }, [v.doc, v.page, v.zoom, v.layout]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = useViewer.getState();
      const mod = e.ctrlKey || e.metaKey;
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
      if (useSign.getState().active && !typing) {
        const sg = useSign.getState();
        if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); if (e.shiftKey) sg.redo(); else sg.undo(); return; }
        if (mod && e.key.toLowerCase() === "y") { e.preventDefault(); sg.redo(); return; }
        if (e.key === "Escape") { if (sg.armed) sg.setArmed(null); else sg.select(null); return; }
        if ((e.key === "Delete" || e.key === "Backspace") && sg.selectedId) { e.preventDefault(); sg.deleteSelected(); return; }
      }
      if (mod && e.key.toLowerCase() === "o") { e.preventDefault(); void openDialog(); }
      else if (mod && e.key.toLowerCase() === "f" && s.doc) { e.preventDefault(); s.setSidebar("search"); }
      else if (mod && e.key.toLowerCase() === "p" && s.doc) { e.preventDefault(); void print(); }
      else if (mod && (e.key === "+" || e.key === "=") && s.doc) { e.preventDefault(); s.setZoom({ kind: "percent", value: stepZoom(Math.round(s.effectiveScale * 100), 1) }); }
      else if (mod && e.key === "-" && s.doc) { e.preventDefault(); s.setZoom({ kind: "percent", value: stepZoom(Math.round(s.effectiveScale * 100), -1) }); }
      else if (mod && e.key === "0" && s.doc) { e.preventDefault(); s.setZoom({ kind: "fit-width" }); }
      else if (e.key === "F4") { e.preventDefault(); s.setSidebar(s.sidebar ? null : "thumbnails"); }
      else if (e.key === "F3" && s.doc) { e.preventDefault(); s.stepHit(e.shiftKey ? -1 : 1); }
      else if (typing || !s.doc) return;
      else if (e.key === "PageDown") s.goToPage(s.page + (s.layout === "two-page" ? 2 : 1));
      else if (e.key === "PageUp") s.goToPage(s.page - (s.layout === "two-page" ? 2 : 1));
      else if (e.key === "Home") s.goToPage(0);
      else if (e.key === "End") s.goToPage(s.doc.pageCount - 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openDialog, print]);

  return (
    <div id="app-root" className="flex h-screen flex-col">
      <Toolbar
        onOpen={openDialog}
        onPrint={print}
        onOrganize={() => v.doc && org.start([v.doc.path])}
        onBulk={() => setBulkOpen(true)}
        onSettings={() => setSettingsOpen(true)}
        onMerge={async () => {
          const paths = await pickPdfs().catch(() => []);
          if (paths.length) await org.start(paths);
        }}
      />

      {libError && (
        <div role="alert" className="border-b px-4 py-2 text-sm" style={{ background: "var(--danger-soft)", color: "var(--danger)", borderColor: "var(--border)" }}>{libError}</div>
      )}
      {v.error && (
        <div role="alert" className="flex items-center justify-between border-b px-4 py-2 text-sm" style={{ background: "color-mix(in srgb, #f59e0b 16%, var(--surface))", borderColor: "var(--border)" }}>
          <span>{v.error.message}</span>
          <button className="btn btn-sm btn-quiet" onClick={() => v.setError(null)}>Dismiss</button>
        </div>
      )}

      {bulkOpen ? (
        <BulkSign onClose={() => setBulkOpen(false)} />
      ) : org.active ? (
        <Organizer onClose={() => org.close()} />
      ) : v.doc ? (
        <div className="flex min-h-0 flex-1">
          {!(narrow && sign.active) && <Sidebar />}
          <Viewer />
          {sign.active && <SignPanel />}
        </div>
      ) : (
        <Welcome key={recentKey} onOpenRecent={openFromRecent} />
      )}

      {settingsOpen && <SettingsDialog onClose={() => setSettingsOpen(false)} onCleared={() => setRecentKey((k) => k + 1)} />}

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
