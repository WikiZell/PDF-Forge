# PDF Forge

Offline, privacy-first desktop PDF reader and editor: view, organise pages, merge/split, Fill &
Sign (draw, type or import a signature) and bulk signing. Rendering by PDFium, shell by Tauri 2,
UI in React + TypeScript. Windows 10/11 x64 is the first target; nothing in the core blocks macOS
or Linux.

* **Offline.** No network calls at runtime, no telemetry, no update checks. The CSP is
  `default-src 'self'` with no remote scripts and no `eval`.
* **Originals are never modified.** Edits go to new files. An explicit "Save (overwrite)" writes
  atomically (temp file + rename) and keeps `<name>.pdf.bak`.
* **PDFs are untrusted.** Size/page limits (configurable in Settings), parser panics contained,
  PDFium work time-boxed, PDF JavaScript never executed, paths validated, output names sanitised.

See [docs/MILESTONES.md](docs/MILESTONES.md) for exactly what is verified (including the Windows cross-build/Wine run and the fuzz results) and what is not.

## Features

| Area | What works |
|---|---|
| Viewer | Open via dialog / drag-drop / CLI arg / file association. Continuous, single and two-page layouts; fit width, fit page, 25–400 %, Ctrl+wheel. Thumbnails, bookmarks, text search with highlights (F3 / Shift+F3), selectable and copyable text, recent files, per-file view position, password prompt (never stored), print. |
| Page organiser | Thumbnail grid; multi-select (click / Shift / Ctrl / Ctrl+A); drag to reorder (pointer), Alt+Arrow to reorder; rotate, delete, duplicate; blank pages (A4, Letter, same size as neighbour); insert pages from another PDF; extract selection; undo/redo. Bookmarks and link annotations are remapped; form fields renamed on collision and **reported**. |
| Merge / split | Merge several PDFs in order with a page-count/size preview. Split every N pages, by page ranges, or at top-level bookmarks. |
| Fill & Sign | Draw (vector, pen pressure), type (4 bundled OFL handwriting fonts) or import PNG/JPEG (background removal with a threshold slider, auto-crop). Library: create, rename, delete. Click-to-place or drag onto a page; move, resize (aspect kept; Shift = free), rotate, delete, keyboard nudge; Date and Text fields; undo/redo; **annotations** (Stamp / Ink / FreeText with appearance streams) or **Flatten on save**. |
| Bulk signing | Choose files or a folder; rule = signature + pages (first/last/all/N) + fixed coordinates or **anchor text** (PDFium search) with offset + optional date/text stamp. Preview (first file rendered, list of files where the anchor is missing), parallel run with progress, cancel, never-overwrite outputs, CSV report. Rules are saved as JSON ([format](docs/bulk-rule-format.md)). |
| Settings | File-size and page limits, privacy statement, **clear all app data**. |

Not implemented (by design, phase 2): certificate-based (PKCS#7/CMS) signatures. The reserved design
is in [docs/signature-embedding.md](docs/signature-embedding.md).

## Architecture

```
crates/pdf-core     pure Rust, no UI/OS deps: page-list model + undo, page-tree rebuild (lopdf),
                    bookmark/link/form remapping, split planning, safe loader, signature image
                    processing, annotation/flatten writer, bulk engine, atomic writes, path allow-list
crates/pdf-render   PDFium wrapper: open, render to PNG, text runs, search, outline
src-tauri           thin Tauri commands + state; OS-specific code only in platform.rs
src/                React UI (Zustand): viewer, sidebar, organiser, sign panel, bulk, settings
e2e/                Playwright tests against the Vite dev server with a mocked IPC layer
fuzz/               cargo-fuzz targets (nightly)
scripts/            fetch-pdfium.mjs (checksum-pinned), gen-licenses.mjs
```

Design points worth knowing:

* **Edits are a page list.** `Project` holds read-only source documents plus an ordered list of
  page references (source page / blank, + extra rotation). Nothing touches a PDF until
  Save/Export, when `build()` copies the sources' objects into a fresh document, builds a new page
  tree and rewrites every object that points at pages. This makes undo trivial and keeps the
  originals intact.
* **Pages render as PNG in Rust**, shown in `<img>`, one bitmap per visible page (lazy, released
  when far off-screen). Text selection uses an invisible text layer from PDFium glyph boxes.
* **Paths are never trusted from the frontend.** Files become readable only after the user picks
  them (dialog, drop, CLI), in Rust; every command re-checks against that allow-list.
* **Dialogs are native** (`tauri-plugin-dialog`), so the frontend never handles filesystem paths
  it chose itself.
* Pointer-based drag-and-drop is used for page reordering because HTML5 DnD is unavailable in
  WebView2 while the OS file-drop handler is enabled.

## Build (Windows 10/11 x64)

Prerequisites: Rust (stable, MSVC toolchain), Node 22+, Visual Studio Build Tools (C++). WebView2
is installed by the installer's bootstrapper if missing.

```powershell
npm ci
npm run fetch-pdfium      # downloads + verifies pdfium.dll into src-tauri\resources\pdfium
npx tauri dev             # run in development
npx tauri build           # MSI + NSIS in target\release\bundle
```

The NSIS installer is **per-user** (no admin rights). The MSI is Tauri's WiX package, which
installs per-machine (known gap, see below). `.pdf` is registered as *openable with* PDF Forge via
`bundle.fileAssociations`; the installer does not make it the default.

### PDFium

PDFium (BSD-3-Clause/Apache-2.0) comes from
[bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries), tag `chromium/8125`,
which must match the `pdfium_latest` feature of `pdfium-render`. `scripts/fetch-pdfium.mjs`
verifies a pinned SHA-256. At startup the app searches the bundled resource folder, then
`PDFIUM_DIR`, then `src-tauri/resources/pdfium`; if none loads it still starts and shows an error
banner listing every location tried.

| Target | Binary | Status |
|---|---|---|
| Windows x64 | `bin/pdfium.dll` | **wired and shipped** |
| Linux x64 | `lib/libpdfium.so` | used for CI tests and local development only |
| macOS x64 / arm64 | `lib/libpdfium.dylib` | not wired |

No PDFium V8 or XFA build is used, so documents cannot run scripts.

## Tests

```bash
npm test                      # Vitest: logic (zoom, layout, selection, placements, stores, rules)
npm run e2e                   # Playwright, mocked IPC (set PLAYWRIGHT_CHROMIUM if needed)
node scripts/fetch-pdfium.mjs linux-x64
PDFIUM_DIR=src-tauri/resources/pdfium cargo test --workspace -- --test-threads=1
cargo test --release -p pdf-core --test perf -- --nocapture   # 1,000-page timings
npm run licenses:check        # THIRD_PARTY_LICENSES current, no disallowed licences
```

Without `PDFIUM_DIR`, PDFium-dependent tests are skipped locally; CI sets `PDFIUM_REQUIRED=1`
so a missing library fails the build. Set `FUZZ_ITERS=5000` for a longer deterministic mutation
run. libFuzzer targets: [fuzz/README.md](fuzz/README.md).

## Decisions and caveats (please review)

* **Dependencies beyond the spec's list:** `tauri-plugin-dialog` (MIT/Apache-2.0; native dialogs),
  `serde`/`serde_json` (MIT/Apache-2.0), and the dev-only `@playwright/test`, `vitest` and `jsdom` (MIT/Apache-2.0). Five transitive Tauri crates are
  MPL-2.0 (file-level copyleft, compatible with permissive distribution). No AGPL/GPL component.
* **Printing** renders pages with PDFium at 150 dpi and hands them to the WebView print dialog
  (`window.print()`), capped at 300 pages per job. Chosen over a native print job for simplicity
  and portability.
* **Typed signatures are raster** (a PNG rendered from the chosen font at high resolution), not
  glyph outlines, so no font-parsing dependency was added.
* **Date stamps use the computer's local date**; non-WinAnsi characters in text fields become `?`.
* **Worker default** is logical CPUs − 1 (std has no physical-core count).
* **Merge** concatenates bookmark trees without a per-file wrapper node. Named destinations are
  resolved into explicit ones. Tagged-PDF structure trees, page labels and XFA are not carried
  over to rebuilt files; Widget annotations are not duplicated on duplicated pages.
* Encrypted inputs are decrypted for editing; **outputs are unencrypted** (a warning is shown).
* Authenticode signing is a documented hook ([docs/RELEASING.md](docs/RELEASING.md)), disabled
  until a certificate exists.
* The app icon is a placeholder.

THIRD_PARTY_LICENSES is generated by `scripts/gen-licenses.mjs`.
