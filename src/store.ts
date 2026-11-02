import { create } from "zustand";
import type { AppError, DocSummary, PageSizePt, SearchHit } from "./lib/api";
import type { Layout } from "./lib/layout";
import { clampPercent, parseZoom, type ZoomMode } from "./lib/zoom";

export type SidebarTab = "thumbnails" | "outline" | "search";

export interface SearchState {
  query: string;
  matchCase: boolean;
  hits: SearchHit[];
  current: number; // index into hits, -1 for none
  running: boolean;
  done: boolean;
}

const emptySearch: SearchState = { query: "", matchCase: false, hits: [], current: -1, running: false, done: false };

interface ViewerState {
  doc: DocSummary | null;
  page: number; // 0-based current page
  /** Bumped whenever the UI asks the viewer to scroll to `page` (not for scroll-driven updates). */
  jumpSeq: number;
  sizes: PageSizePt[];
  zoom: ZoomMode;
  /** Actual CSS px per point as laid out by the viewer (differs from `zoom` in fit modes). */
  effectiveScale: number;
  layout: Layout;
  sidebar: SidebarTab | null;
  search: SearchState;
  error: AppError | null;
  /** Path waiting for a password; the password itself is never stored. */
  pendingPasswordPath: string | null;
  busy: boolean;

  setDoc: (doc: DocSummary | null) => void;
  /** Navigate: updates the page and asks the viewer to scroll. */
  goToPage: (page: number) => void;
  /** Scroll-driven page update; does not trigger a scroll. */
  setPageFromScroll: (page: number) => void;
  setSizes: (s: PageSizePt[]) => void;
  setZoom: (z: ZoomMode) => void;
  setEffectiveScale: (n: number) => void;
  setLayout: (l: Layout) => void;
  setSidebar: (t: SidebarTab | null) => void;
  setSearch: (s: Partial<SearchState>) => void;
  stepHit: (dir: 1 | -1) => void;
  setError: (e: AppError | null) => void;
  setPendingPassword: (path: string | null) => void;
  setBusy: (b: boolean) => void;
}

const clampPage = (doc: DocSummary | null, page: number) =>
  doc && Number.isFinite(page) ? Math.min(doc.pageCount - 1, Math.max(0, Math.trunc(page))) : 0;

const LAYOUTS: Layout[] = ["single", "continuous", "two-page"];

export const useViewer = create<ViewerState>((set, get) => ({
  doc: null,
  page: 0,
  jumpSeq: 0,
  sizes: [],
  zoom: { kind: "fit-width" },
  effectiveScale: 1,
  layout: "continuous",
  sidebar: "thumbnails",
  search: emptySearch,
  error: null,
  pendingPasswordPath: null,
  busy: false,

  setDoc: (doc) => {
    const vs = doc?.viewState;
    set((s) => ({
      doc,
      page: clampPage(doc, vs?.page ?? 0),
      jumpSeq: s.jumpSeq + 1,
      sizes: [],
      zoom: vs ? parseZoom(vs.zoom) : s.zoom,
      layout: vs && LAYOUTS.includes(vs.layout as Layout) ? (vs.layout as Layout) : s.layout,
      search: emptySearch,
      error: null,
    }));
  },
  goToPage: (page) => {
    const doc = get().doc;
    if (!doc || !Number.isFinite(page)) return;
    set((s) => ({ page: clampPage(doc, page), jumpSeq: s.jumpSeq + 1 }));
  },
  setPageFromScroll: (page) => set({ page: clampPage(get().doc, page) }),
  setSizes: (sizes) => set({ sizes }),
  setZoom: (z) => set({ zoom: z.kind === "percent" ? { kind: "percent", value: clampPercent(z.value) } : z }),
  setEffectiveScale: (effectiveScale) => set({ effectiveScale }),
  setLayout: (layout) => set((s) => ({ layout, jumpSeq: s.jumpSeq + 1 })),
  setSidebar: (sidebar) => set({ sidebar }),
  setSearch: (p) => set((s) => ({ search: { ...s.search, ...p } })),
  stepHit: (dir) => {
    const { search, doc } = get();
    if (!doc || search.hits.length === 0) return;
    const n = search.hits.length;
    const current = search.current < 0 ? (dir > 0 ? 0 : n - 1) : (search.current + dir + n) % n;
    set((s) => ({ search: { ...s.search, current }, page: search.hits[current].page, jumpSeq: s.jumpSeq + 1 }));
  },
  setError: (error) => set({ error }),
  setPendingPassword: (pendingPasswordPath) => set({ pendingPasswordPath }),
  setBusy: (busy) => set({ busy }),
}));
