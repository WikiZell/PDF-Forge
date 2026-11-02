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
    | "internal";
  message: string;
}

export interface DocSummary {
  id: number;
  fileName: string;
  pageCount: number;
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
