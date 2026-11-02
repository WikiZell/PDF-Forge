# Milestone 1 report: scaffold

## Works
- Cargo workspace (`pdf-core`, `pdf-render`, `src-tauri`) and Vite/React/Tailwind/Zustand frontend.
- Open a PDF via native dialog (Ctrl+O), drag-and-drop onto the window, or CLI argument; single page displayed, page navigation, fit width / fit page / 25-400% zoom, password prompt, error banners.
- PDFium load at startup with a clear banner listing searched locations if missing.
- Path allow-list, size and page limits, file-name sanitiser; strict CSP.
- CI: Linux tests with real PDFium; Windows job builds unsigned MSI + NSIS.

## Tested
- `cargo test`: 9 `pdf-core` unit tests, 4 `pdf-render` tests against real PDFium (page count, rotation, 2x render size, out-of-range, oversize, garbage file, missing library). Run on Linux x64.
- `vitest`: 9 tests (zoom math, store clamping).
- `tsc`, `vite build` and `cargo check -p pdf-forge` succeed on Linux.

## Not verified
- Windows build, installers, and the running GUI have **not** been run (no Windows machine here); the first CI run on `windows-latest` is the real check.
- Password-protected file path is wired but has no encrypted fixture yet (Milestone 4 fixtures).
- No Playwright/WebDriver E2E yet.
- High-DPI at 100/125/150/200% not yet tested.

## Known gaps / next
- Placeholder icon; `.msi` file association and WebView2 settings need confirmation on a real build.
- `lopdf` pre-check is limited to the page limit for now.
