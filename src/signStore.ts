import { create } from "zustand";
import { toAppError, type AppError } from "./lib/api";
import {
  commit, initHistory, redo as hRedo, undo as hUndo, type History, type Placement,
} from "./lib/placements";
import * as api from "./lib/sig-api";

export type Armed = { kind: "signature"; sigId: string } | { kind: "date" } | { kind: "text" } | null;

interface SignState {
  active: boolean;
  library: api.SigMeta[];
  history: History<Placement[]>;
  selectedId: string | null;
  armed: Armed;
  flatten: boolean;
  error: AppError | null;
  notice: string[] | null;
  busy: boolean;

  setActive: (a: boolean) => void;
  loadLibrary: () => Promise<void>;
  addSignature: (m: api.SigMeta) => void;
  rename: (id: string, name: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  setArmed: (a: Armed) => void;
  select: (id: string | null) => void;
  /** Replace the list as a new undo step. */
  commitPlacements: (next: Placement[]) => void;
  /** Replace without creating an undo step (during a drag). */
  liveUpdate: (next: Placement[]) => void;
  /** Push the state captured at drag start as the undo step once the drag ends. */
  endDrag: (before: Placement[]) => void;
  undo: () => void;
  redo: () => void;
  deleteSelected: () => void;
  setFlatten: (f: boolean) => void;
  setError: (e: AppError | null) => void;
  setNotice: (n: string[] | null) => void;
  setBusy: (b: boolean) => void;
  reset: () => void;
}

export const useSign = create<SignState>((set, get) => ({
  active: false,
  library: [],
  history: initHistory<Placement[]>([]),
  selectedId: null,
  armed: null,
  flatten: false,
  error: null,
  notice: null,
  busy: false,

  setActive: (active) => set({ active, armed: null, selectedId: null }),
  loadLibrary: async () => {
    try {
      set({ library: await api.sigList() });
    } catch (e) {
      set({ error: toAppError(e) });
    }
  },
  addSignature: (m) => set((s) => ({ library: [...s.library, m] })),
  rename: async (id, name) => {
    try {
      await api.sigRename(id, name);
      await get().loadLibrary();
    } catch (e) {
      set({ error: toAppError(e) });
    }
  },
  remove: async (id) => {
    try {
      await api.sigDelete(id);
      await get().loadLibrary();
    } catch (e) {
      set({ error: toAppError(e) });
    }
  },
  setArmed: (armed) => set({ armed, selectedId: armed ? null : get().selectedId }),
  select: (selectedId) => set({ selectedId }),
  commitPlacements: (next) => set((s) => ({ history: commit(s.history, next) })),
  liveUpdate: (next) => set((s) => ({ history: { ...s.history, present: next } })),
  endDrag: (before) =>
    set((s) => {
      if (JSON.stringify(before) === JSON.stringify(s.history.present)) return s;
      return { history: { past: [...s.history.past, before].slice(-200), present: s.history.present, future: [] } };
    }),
  undo: () => set((s) => ({ history: hUndo(s.history), selectedId: null })),
  redo: () => set((s) => ({ history: hRedo(s.history), selectedId: null })),
  deleteSelected: () => {
    const { selectedId, history } = get();
    if (!selectedId) return;
    get().commitPlacements(history.present.filter((p) => p.id !== selectedId));
    set({ selectedId: null });
  },
  setFlatten: (flatten) => set({ flatten }),
  setError: (error) => set({ error }),
  setNotice: (notice) => set({ notice }),
  setBusy: (busy) => set({ busy }),
  reset: () => set({ history: initHistory<Placement[]>([]), selectedId: null, armed: null, notice: null, error: null }),
}));

export const placementsOf = () => useSign.getState().history.present;
