import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface AppError {
  code:
    | "library_missing"
    | "password_required"
    | "wrong_password"
    | "not_allowed"
    | "too_large"
    | "too_many_pages"
    | "open_failed"
    | "render_failed"
    | "no_such_document"
    | "timeout"
    | "internal";
  message: string;
}

export interface DocSummary {
  id: number;
  path: string;
  fileName: string;
  pageCount: number;
  viewState: ViewStateDto | null;
}

export interface PageSizePt {
  width: number;
  height: number;
}

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    return e as AppError;
  }
  return { code: "internal", message: typeof e === "string" ? e : "Unexpected error." };
}

/** Status of the bundled PDFium library, so the UI can show a clear message. */
export const pdfiumStatus = () => invoke<{ ok: boolean; message: string | null }>("pdfium_status");

/** Shows the native file dialog (in Rust). Resolves with the chosen path or null. */
export const pickPdf = () => invoke<string | null>("pick_pdf");

/** A path delivered by the OS at launch (CLI arg / file association), if any. */
export const takeLaunchPath = () => invoke<string | null>("take_launch_path");

export const openPdf = (path: string, password?: string) =>
  invoke<DocSummary>("open_pdf", { path, password: password ?? null });

export const closePdf = (id: number) => invoke<void>("close_pdf", { id });

export const pageSize = (id: number, page: number) =>
  invoke<PageSizePt>("page_size", { id, page });

/** Returns PNG bytes for one page. `scale` is device pixels per PDF point. */
export async function renderPage(id: number, page: number, scale: number): Promise<Blob> {
  const buf = await invoke<ArrayBuffer>("render_page", { id, page, scale });
  return new Blob([buf], { type: "image/png" });
}

/** Paths the user dropped on the window or opened via the OS; granted by Rust before emit. */
export const onOpenRequest = (cb: (path: string) => void): Promise<UnlistenFn> =>
  listen<string>("open-request", (e) => cb(e.payload));

export interface TextRun { text: string; rect: Rect }
export interface Rect { x: number; y: number; w: number; h: number }
export interface OutlineItem { title: string; page: number | null; depth: number }
export interface SearchHit { page: number; rects: Rect[] }
export interface RecentFile { path: string; name: string; openedAt: number }
export interface ViewStateDto { page: number; zoom: string; layout: string }

export const pageSizes = (id: number) => invoke<PageSizePt[]>("page_sizes", { id });
export const pageText = (id: number, page: number) => invoke<TextRun[]>("page_text", { id, page });
export const outline = (id: number) => invoke<OutlineItem[]>("outline", { id });
export const searchDoc = (id: number, query: string, matchCase: boolean) =>
  invoke<SearchHit[]>("search", { id, query, matchCase });
export const recentFiles = () => invoke<RecentFile[]>("recent_files");
export const openRecent = (path: string) => invoke<string>("open_recent", { path });
export const forgetRecent = (path: string) => invoke<void>("forget_recent", { path });
export const saveViewState = (path: string, view: ViewStateDto) =>
  invoke<void>("save_view_state", { path, view });
export const clearAppData = () => invoke<void>("clear_app_data");

export interface Settings { maxFileSizeMb: number; maxPages: number }
export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (settings: Settings) => invoke<Settings>("set_settings", { settings });
