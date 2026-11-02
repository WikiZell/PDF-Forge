import type { Page } from "@playwright/test";

/**
 * Installs a fake Tauri IPC layer before the app loads. It implements just enough of the backend
 * (open/render/organize/sign/bulk commands) with in-memory state to exercise the UI flows.
 * Every call is recorded on `window.__calls` so tests can assert what the UI asked the backend to do.
 */
export async function installMock(page: Page, opts: { password?: string; library?: boolean } = {}) {
  await page.addInitScript((o) => {
    const png = Uint8Array.from(atob("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="), (c) => c.charCodeAt(0));
    const calls: { cmd: string; args: any }[] = [];
    (window as any).__calls = calls;
    const sizes = [{ width: 300, height: 400 }, { width: 400, height: 300 }, { width: 300, height: 400 }];
    let pages = [0, 1, 2].map((i) => ({ kind: "source", src: 0, page: i, rotate: 0, width: sizes[i].width, height: sizes[i].height }));
    const past: any[] = [];
    const future: any[] = [];
    const lib: any[] = o.library ? [{ id: "abc0", name: "My signature", kind: "drawn", created: 0, aspect: 3, drawn: { strokes: [[[0, 0, 0.5], [30, 10, 0.5]]], view: [30, 10], width: 2, color: [0, 0, 0] } }] : [];
    const state = () => ({
      session: 1, pages, sources: [{ name: "a.pdf", pageCount: 3 }],
      canUndo: past.length > 0, canRedo: future.length > 0, canSaveOver: true,
    });
    const snap = () => { past.push(JSON.parse(JSON.stringify(pages))); future.length = 0; };
    const callbacks: Record<number, Function> = {};
    let cb = 1;

    const handle = (cmd: string, a: any): any => {
      switch (cmd) {
        case "pdfium_status": return { ok: true, message: null };
        case "take_launch_path": return null;
        case "recent_files": return [];
        case "pick_pdf": return "/mock/a.pdf";
        case "open_pdf":
          if (o.password && a.password !== o.password) throw { code: a.password ? "wrong_password" : "password_required", message: "needs password" };
          return { id: 1, path: a.path, fileName: "a.pdf", pageCount: 3, viewState: null };
        case "page_sizes": return sizes;
        case "page_size": return sizes[a.page];
        case "render_page": case "org_thumb": return png.buffer;
        case "page_text": return [{ text: "Hello World", rect: { x: 20, y: 20, w: 100, h: 14 } }];
        case "outline": return [{ title: "Chapter 1", page: 0, depth: 0 }, { title: "Section", page: 2, depth: 1 }];
        case "search": return a.query === "hello" ? [{ page: 0, rects: [{ x: 20, y: 20, w: 100, h: 14 }] }, { page: 2, rects: [{ x: 5, y: 5, w: 50, h: 12 }] }] : [];
        case "close_pdf": case "save_view_state": case "forget_recent": return null;
        case "org_new": return state();
        case "org_add_file": return state();
        case "org_close": return null;
        case "org_edit": {
          const e = a.edit;
          if (e.op === "undo") { if (past.length) { future.push(JSON.parse(JSON.stringify(pages))); pages = past.pop(); } return state(); }
          if (e.op === "redo") { if (future.length) { past.push(JSON.parse(JSON.stringify(pages))); pages = future.pop(); } return state(); }
          snap();
          const sel = [...(e.indices ?? [])].sort((x: number, y: number) => x - y);
          if (e.op === "rotate") sel.forEach((i: number) => (pages[i].rotate = (pages[i].rotate + e.degrees + 360) % 360));
          if (e.op === "delete") pages = pages.filter((_, i) => !sel.includes(i));
          if (e.op === "duplicate") { const at = sel[sel.length - 1] + 1; pages.splice(at, 0, ...sel.map((i: number) => ({ ...pages[i] }))); }
          if (e.op === "move") { const moved = sel.map((i: number) => pages[i]); const rest = pages.filter((_, i) => !sel.includes(i)); rest.splice(e.to, 0, ...moved); pages = rest; }
          if (e.op === "insertBlank") pages.splice(e.at, 0, { kind: "blank", src: 0, page: 0, rotate: 0, width: 595, height: 842 });
          return state();
        }
        case "org_preview": return { pageCount: pages.length, sizeBytes: 2048, renamedFields: [{ source: "b.pdf", from: "name", to: "name_2" }], warnings: [], droppedLinks: 0, droppedBookmarks: 0 };
        case "get_settings": return { maxFileSizeMb: 2048, maxPages: 100000 };
        case "set_settings": if (a.settings.maxFileSizeMb < 1) throw { code: "invalid_edit", message: "Maximum file size must be between 1 and 16384 MB." }; return a.settings;
        case "clear_app_data": lib.length = 0; return null;
        case "pick_pdfs": return ["/mock/b.pdf"];
        case "sig_list": return lib;
        case "sig_create_drawn": { const m = { id: "abc" + lib.length, name: a.d.name, kind: "drawn", created: 0, aspect: a.d.view[0] / a.d.view[1], drawn: { strokes: a.d.strokes, view: a.d.view, width: a.d.width, color: a.d.color } }; lib.push(m); return m; }
        case "sign_document": return { path: "/mock/a_signed.pdf", applied: a.placements.length, warnings: [] };
        case "bulk_default_workers": return 3;
        case "bulk_pick_folder": return ["/mock/c1.pdf", "/mock/c2.pdf"];
        case "bulk_pick_output_dir": return "/mock/out";
        case "bulk_preview": return a.files.map((f: string, i: number) => i === 0 ? { input: f, status: "signed", detail: "1 signature(s)", output: null } : { input: f, status: "skipped", detail: 'anchor text "Signature:" not found', output: null });
        case "bulk_preview_image": return png.buffer;
        case "bulk_run": return 7;
        default:
          if (cmd.startsWith("plugin:event|listen")) return cb++;
          if (cmd.startsWith("plugin:event|")) return null;
          throw { code: "internal", message: "mock: unhandled " + cmd };
      }
    };
    (window as any).__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
      transformCallback: (f: Function) => { const id = cb++; callbacks[id] = f; return id; },
      unregisterCallback: (id: number) => { delete callbacks[id]; },
      convertFileSrc: (p: string) => p,
      invoke: async (cmd: string, args: any = {}) => {
        calls.push({ cmd, args: JSON.parse(JSON.stringify(args)) });
        const r = handle(cmd, args);
        // Real IPC serialises results; never hand the app a reference into the mock's own state.
        return r instanceof ArrayBuffer || r === null || r === undefined ? r : JSON.parse(JSON.stringify(r));
      },
    };
    (window as any).__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  }, opts);
}

export const calls = (page: Page) => page.evaluate(() => (window as any).__calls as { cmd: string; args: any }[]);
