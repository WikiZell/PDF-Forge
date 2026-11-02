import { create } from "zustand";
import type { AppError, DocSummary, PageSizePt } from "./lib/api";
import { clampPercent, type ZoomMode } from "./lib/zoom";

interface ViewerState {
  doc: DocSummary | null;
  page: number; // 0-based
  pageSize: PageSizePt | null;
  zoom: ZoomMode;
  error: AppError | null;
  /** Path waiting for a password; the password itself is never stored. */
  pendingPasswordPath: string | null;
  busy: boolean;

  setDoc: (doc: DocSummary | null) => void;
  goToPage: (page: number) => void;
  setPageSize: (s: PageSizePt | null) => void;
  setZoom: (z: ZoomMode) => void;
  setError: (e: AppError | null) => void;
  setPendingPassword: (path: string | null) => void;
  setBusy: (b: boolean) => void;
}

export const useViewer = create<ViewerState>((set, get) => ({
  doc: null,
  page: 0,
  pageSize: null,
  zoom: { kind: "fit-width" },
  error: null,
  pendingPasswordPath: null,
  busy: false,

  setDoc: (doc) => set({ doc, page: 0, pageSize: null, error: null }),
  goToPage: (page) => {
    const doc = get().doc;
    if (!doc || !Number.isFinite(page)) return;
    set({ page: Math.min(doc.pageCount - 1, Math.max(0, Math.trunc(page))) });
  },
  setPageSize: (pageSize) => set({ pageSize }),
  setZoom: (z) => set({ zoom: z.kind === "percent" ? { kind: "percent", value: clampPercent(z.value) } : z }),
  setError: (error) => set({ error }),
  setPendingPassword: (pendingPasswordPath) => set({ pendingPasswordPath }),
  setBusy: (busy) => set({ busy }),
}));
