import { create } from "zustand";
import * as api from "./lib/organize-api";
import { toAppError, type AppError } from "./lib/api";
import { clamp, click, empty, selectAll, type Selection } from "./lib/selection";

interface OrganizerState {
  active: boolean;
  state: api.OrgState | null;
  sel: Selection;
  error: AppError | null;
  notice: string[] | null;
  busy: boolean;
  preview: api.Preview | null;
  /** File waiting for a password: path and the insert position (undefined = append). */
  pendingPassword: { path: string; insertAt?: number; wrong: boolean } | null;

  start: (paths: string[]) => Promise<void>;
  addFile: (path: string, password?: string, insertAt?: number) => Promise<void>;
  edit: (e: api.Edit, nextSel?: (count: number) => Selection) => Promise<void>;
  select: (index: number, mods: { shift: boolean; ctrl: boolean }) => void;
  setSel: (s: Selection) => void;
  selectAll: () => void;
  refreshPreview: (subset?: number[]) => Promise<void>;
  close: () => Promise<void>;
  setError: (e: AppError | null) => void;
  setNotice: (n: string[] | null) => void;
  setPendingPassword: (p: OrganizerState["pendingPassword"]) => void;
  setBusy: (b: boolean) => void;
}

export const useOrganizer = create<OrganizerState>((set, get) => ({
  active: false,
  state: null,
  sel: empty,
  error: null,
  notice: null,
  busy: false,
  preview: null,
  pendingPassword: null,

  start: async (paths) => {
    set({ busy: true, error: null, notice: null, preview: null });
    try {
      const s = await api.orgNew();
      set({ active: true, state: s, sel: empty });
      for (const p of paths) await get().addFile(p);
    } catch (e) {
      set({ error: toAppError(e) });
    } finally {
      set({ busy: false });
    }
  },

  addFile: async (path, password, insertAt) => {
    const cur = get().state;
    if (!cur) return;
    set({ busy: true, error: null });
    try {
      const s = await api.orgAddFile(cur.session, path, password, insertAt);
      set({ state: s, sel: clamp(get().sel, s.pages.length), pendingPassword: null, preview: null });
    } catch (e) {
      const err = toAppError(e);
      if (err.code === "password_required" || err.code === "wrong_password") {
        set({ pendingPassword: { path, insertAt, wrong: err.code === "wrong_password" } });
      } else {
        set({ error: err, pendingPassword: null });
      }
    } finally {
      set({ busy: false });
    }
  },

  edit: async (e, nextSel) => {
    const cur = get().state;
    if (!cur) return;
    try {
      const s = await api.orgEdit(cur.session, e);
      set({ state: s, error: null, preview: null, sel: nextSel ? nextSel(s.pages.length) : clamp(get().sel, s.pages.length) });
    } catch (err) {
      set({ error: toAppError(err) });
    }
  },

  select: (index, mods) => {
    const n = get().state?.pages.length ?? 0;
    set({ sel: click(get().sel, index, mods, n) });
  },
  setSel: (sel) => set({ sel }),
  selectAll: () => set({ sel: selectAll(get().state?.pages.length ?? 0) }),

  refreshPreview: async (subset) => {
    const cur = get().state;
    if (!cur) return;
    set({ busy: true });
    try {
      set({ preview: await api.orgPreview(cur.session, subset), error: null });
    } catch (e) {
      set({ error: toAppError(e) });
    } finally {
      set({ busy: false });
    }
  },

  close: async () => {
    const cur = get().state;
    if (cur) await api.orgClose(cur.session).catch(() => {});
    set({ active: false, state: null, sel: empty, error: null, notice: null, preview: null, pendingPassword: null });
  },

  setError: (error) => set({ error }),
  setNotice: (notice) => set({ notice }),
  setPendingPassword: (pendingPassword) => set({ pendingPassword }),
  setBusy: (busy) => set({ busy }),
}));
