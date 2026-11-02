import { invoke } from "@tauri-apps/api/core";

export interface OrgPage {
  kind: "source" | "blank";
  src: number;
  page: number;
  rotate: number;
  width: number;
  height: number;
}
export interface OrgState {
  session: number;
  pages: OrgPage[];
  sources: { name: string; pageCount: number }[];
  canUndo: boolean;
  canRedo: boolean;
  canSaveOver: boolean;
}
export interface Preview {
  pageCount: number;
  sizeBytes: number;
  renamedFields: { source: string; from: string; to: string }[];
  warnings: string[];
  droppedLinks: number;
  droppedBookmarks: number;
}
export interface Saved {
  path: string;
  preview: Preview;
}
export interface SplitResult {
  folder: string;
  files: string[];
}

export type Edit =
  | { op: "move"; indices: number[]; to: number }
  | { op: "rotate"; indices: number[]; degrees: number }
  | { op: "delete"; indices: number[] }
  | { op: "duplicate"; indices: number[] }
  | { op: "insertBlank"; at: number; size: "a4" | "letter" | "same"; like?: number }
  | { op: "insertFrom"; src: number; pages?: number[]; at: number }
  | { op: "undo" }
  | { op: "redo" };

export type SplitMode =
  | { mode: "everyN"; n: number }
  | { mode: "ranges"; spec: string }
  | { mode: "bookmarks" };

export const orgNew = () => invoke<OrgState>("org_new");
export const orgAddFile = (session: number, path: string, password?: string, insertAt?: number) =>
  invoke<OrgState>("org_add_file", { session, path, password: password ?? null, insertAt: insertAt ?? null });
export const orgEdit = (session: number, edit: Edit) => invoke<OrgState>("org_edit", { session, edit });
export const orgPreview = (session: number, subset?: number[]) =>
  invoke<Preview>("org_preview", { session, subset: subset ?? null });
export const orgSaveAs = (session: number, defaultName: string, subset?: number[]) =>
  invoke<Saved | null>("org_save_as", { session, subset: subset ?? null, defaultName });
export const orgSaveOver = (session: number) => invoke<Saved>("org_save_over", { session });
export const orgSplit = (session: number, mode: SplitMode) =>
  invoke<SplitResult | null>("org_split", { session, mode });
export const pickPdfs = () => invoke<string[]>("pick_pdfs");
export const orgClose = (session: number) => invoke<void>("org_close", { session });
export async function orgThumb(session: number, src: number, page: number, scale: number): Promise<Blob> {
  const buf = await invoke<ArrayBuffer>("org_thumb", { session, src, page, scale });
  return new Blob([buf], { type: "image/png" });
}

export const formatBytes = (n: number) =>
  n < 1024 ? `${n} B` : n < 1024 ** 2 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1024 ** 2).toFixed(1)} MB`;
