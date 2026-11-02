# PDF Forge

Offline, privacy-first desktop PDF reader and editor. Rendering by PDFium, shell by Tauri 2,
UI in React + TypeScript. Windows 10/11 x64 first; the code is structured to port to macOS and Linux.

**Status: Milestone 1 (scaffold).** A PDF opens and page 1+ display, with page navigation and
zoom. Everything else in the spec comes in later milestones.

## Privacy and safety rules (enforced in code)
- No network access at runtime: CSP is `default-src 'self'`, no remote scripts, no telemetry, no update checks.
- The frontend never supplies trusted paths. Files are granted in Rust (native dialog, OS drag-and-drop,
  CLI argument) and every command re-checks the path against that allow-list (`pdf-core::AllowedPaths`).
- Size limit (default 2 GB) and page limit (default 100,000) apply to every opened file.
- PDF JavaScript and XFA are not compiled into PDFium use (`pdfium-render` features `pdfium_enable_v8`/`xfa` are off).
- Passwords are used for the single `open_pdf` call and never stored.
- Input files are opened read-only; nothing writes to the original.

## Layout
| Path | Purpose |
|---|---|
| `crates/pdf-core` | Pure Rust: path allow-list, limits, file-name sanitising, structure inspection (lopdf). No UI/OS deps. |
| `crates/pdf-render` | PDFium wrapper: open, page size, render page to PNG. |
| `src-tauri` | Thin Tauri commands, state, drag-drop/CLI handling. OS-specific code only in `platform.rs`. |
| `src` | React UI (Zustand store, Tailwind). |
| `scripts/fetch-pdfium.mjs` | Downloads the pinned PDFium build (checksum-verified) per target. |

## Build on Windows
Prerequisites: Rust (stable, MSVC toolchain), Node 22+, and the WebView2 runtime (bundled bootstrapper in installers).

```powershell
npm ci
npm run fetch-pdfium          # downloads pdfium.dll into src-tauri/resources/pdfium
npx tauri dev                 # run
npx tauri build               # MSI + NSIS in target/release/bundle
```

## PDFium
PDFium (BSD-3-Clause) binaries come from [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries),
tag `chromium/8125`, which must match the `pdfium_latest` feature of `pdfium-render`.
If the library is missing the app still starts and shows an error banner listing every location searched.

| Target | Binary | Status |
|---|---|---|
| Windows x64 | `bin/pdfium.dll` | shipped |
| Linux x64 | `lib/libpdfium.so` | used for CI tests only |
| macOS x64/arm64 | `lib/libpdfium.dylib` | not wired |

## Tests
```bash
npm test                                   # Vitest
PDFIUM_DIR=src-tauri/resources/pdfium cargo test -p pdf-core -p pdf-render -- --test-threads=1
```
Without `PDFIUM_DIR` the render tests skip locally; CI sets `PDFIUM_REQUIRED=1` so a missing library fails the build.

## Decisions to review
- **Extra dependency:** `tauri-plugin-dialog` (MIT/Apache-2.0) for the native open dialog; it is not in the original list. Everything else is Tauri/React/Zustand/Tailwind/lopdf/pdfium-render/image as specified.
- Pages render to PNG in Rust and display in an `<img>`; one page at a time for now (continuous layout in Milestone 2).
- Authenticode signing is a disabled hook in CI (`SIGN_WINDOWS`), pending a certificate.
- Placeholder app icon; replace `src-tauri/icons`.
