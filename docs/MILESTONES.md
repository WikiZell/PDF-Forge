# Milestone reports

Legend: **verified** = covered by an automated test that ran; **observed** = exercised by hand in the
running app under a virtual display on Linux (WebKitGTK); **not verified** = not run anywhere yet.

## Windows status (honest summary)
Done here: the whole app cross-compiles to a Windows x64 release `pdf-forge.exe` (PE32+ GUI, CSP
embedded) with `x86_64-pc-windows-gnu`, and the entire `pdf-core` + `pdf-render` test suite
(104 tests: signing geometry, bulk, merge/organise, encryption, fuzz regressions, perf) was run as
**Windows executables under Wine against the real Windows `pdfium.dll`** — all pass.
Cross-testing found one real Windows-only bug (bulk output publishing used create-then-rename,
which fails with "Access denied" on Windows semantics); it now publishes with a temp file + hard link
(`write_new_atomic`), covered by tests.
**Not done:** running the GUI on WebView2, and producing the MSI/NSIS installers — NSIS bundling
stopped because this sandbox's proxy blocks `go.microsoft.com` (WebView2 bootstrapper download), and
WiX needs Windows. The `windows-latest` CI job does both; its first run is still the acceptance
test for the installers. Also never opened in Acrobat Reader / Chrome's viewer.

## Fuzzing
`cargo +nightly fuzz run load_build` (load → edit → rebuild → sign annotate/flatten) found a real
hang within minutes: lopdf's `prune_objects` is exponential on shared/cyclic object graphs
(timeout >20 s on a 1 KB input). Replaced by a linear visited-set prune in `pdf-core::build`; the
input is kept as `tests/fixtures/fuzz-regressions/` and asserted by a test. After the fix:
**118,956 runs / 301 s with no crash, hang or OOM**, and `split_ranges` ran 885,522 times clean.
This is a first pass, not an exhaustive one; CI runs a 2-minute pass on every push.

## 1. Scaffold
Works: Tauri 2 + React/Vite/Tailwind/Zustand, workspace crates, PDFium loading with a clear error
banner, checksum-pinned PDFium fetch, CI (Linux tests; Windows MSI+NSIS build).
Verified: render/size/rotation/limits against real PDFium; path allow-list, size limit, file-name
sanitiser. Observed: open from CLI arg and render.

## 2. Viewer
Works: three layouts, zoom, lazy rendering, text layer, thumbnails, bookmarks, search, recent
files, view state, password prompt, print.
Verified: text boxes through page rotation, outline pages, search (case/pages), 1,000-page timings
(load 6 ms, rebuild 40 ms, open in PDFium <1 ms, render 4 pages 10 ms, search 9 ms, 36 MB peak — a
**text-only** fixture, so real scanned files will be much heavier), 18 Playwright flows.
Observed: continuous layout, thumbnails, rotated page.
Gaps: print (`window.print()` path) not exercised; link annotations in the viewer are not clickable
(so no external-URL confirmation is needed yet); selection/copy tested only for DOM presence.

## 3. Page organisation
Works: everything in the README table. Verified: reorder / delete / duplicate / rotate / blank /
insert / extract with **explicit outline and link remapping tests** (explicit `/Dest`, GoTo action,
named destination; deleted targets drop links and promote child bookmarks), undo/redo, form-field
removal and collision renaming, and a PDFium cross-check (an independent parser confirms page
order, bookmark targets and text). Observed: select, rotate, pointer drag-reorder, undo.
Gaps: save dialogs not driven in the GUI (the write path is covered by core tests).

## 4. Merge / split / insert / extract
Verified: merge of 3 documents with colliding field names (every rename reported), split by N /
ranges / bookmarks, encrypted (AES-128, RC4) with right/wrong/no password, corrupt/truncated
inputs, 6,000-iteration deterministic mutation fuzz (load → edit → rebuild → sign) with zero panics.
Gaps: no AES-256 (V5/R6) fixture. See "Fuzzing" above for the libFuzzer results.

## 5. Signatures
Verified (by rendering with PDFium): placement at the exact requested rectangle for page rotation
0/90/180/270, CropBox offset, rotated placements, alpha, ink, text; annotate and flatten modes;
originals' content/links preserved. Observed: draw → library → place → resize/rotate/undo → save
signed copy through the native dialog → reopened with the signature at the right place.
Gaps: Type and Import tabs only covered by unit tests (image processing) and mocked E2E; not
compared against Acrobat/Chrome rendering; pressure sensitivity untested with a real pen.

## 6. Bulk signing
Verified (9 integration tests, real PDFium anchor search): 24 files in parallel with originals
byte-identical, anchor placement relative to found text, first/last/all pages, date stamps,
never-overwrite (including parallel same-name files), cancel, corrupt/empty/missing/encrypted/short
files reported per file, CSV injection guard, rule validation.
Observed: UI renders; add-folder reported a clear error on an invalid selection.
**Not verified:** the progress/finished *events* in the real window (GTK dialog automation was
unreliable; the E2E mock covers the UI side only).

## 7. Hardening
Done: configurable limits, render/open timeouts, panic containment, fuzz harness (run, bug fixed), licence inventory
with a disallowed-licence gate, modal focus management (found by an E2E test), clippy clean in
shipped code, formatted. Not done: accessibility audit with a real screen reader; DPI testing; a
heavy real-world 1,000-page profile; installer polish; the MSI is per-machine.
