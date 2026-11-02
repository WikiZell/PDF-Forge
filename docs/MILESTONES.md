# Milestone reports

Legend: **verified** = covered by an automated test that ran; **observed** = exercised by hand in the
running app under a virtual display on Linux (WebKitGTK); **not verified** = not run anywhere yet.

## Windows status (honest summary)
**CI on GitHub's runners (run 41489197936, all green):** on `windows-latest` the `pdf-core` and
`pdf-render` tests pass against the real Windows `pdfium.dll`, and `tauri build` produced the
unsigned **MSI and NSIS installers** (artifact `windows-installers`, 18.8 MB). The Linux job (typecheck,
unit tests, 18 Playwright flows, fmt, licence gate, Rust tests with PDFium, 1,000-page perf, clippy
`-D warnings`, Tauri crate tests) and a 2-minute libFuzzer pass also passed.
Earlier, locally: the app was cross-compiled to a Windows `pdf-forge.exe` and the 104 core tests were
run as Windows executables under Wine; that cross-testing found a Windows-only bug in bulk output
publishing (fixed with temp file + hard link, `write_new_atomic`).
CI also caught that the workflow YAML itself was invalid (unquoted colon in a job name), which meant
CI had never run; fixed.

**Installed app on a real Windows desktop (CI job "Windows smoke", run 41490630260):** the NSIS installer
installed silently per user (no admin) to `%LOCALAPPDATA%\PDF Forge\pdf-forge.exe`; the installed app
was launched on three PDFs (merged/reordered, signed with annotations, signed flattened) on a Windows 11
desktop with WebView2, stayed running, and desktop screenshots show the "PDF Forge" window rendering
them correctly (signature image, ink stroke, text, thumbnails, 5-page merged document with a rotated
page). The `windows-smoke` branch holds those screenshots and sample PDFs.

**Interactive E2E against the real installed app on Windows (CI, `e2e-real/`, all 7 pass):** Playwright
attaches to the installed app's WebView2 over CDP (enabled on the CI runner only via the WebView2 policy
registry key; the product has no debug port), so every click goes through the real Rust backend, real
PDFium and real storage. Covered: open from the command line with page bitmaps and thumbnails really
rendered; no network requests leave the app; text search with highlights (incl. a rotated page),
bookmarks, zoom, layouts; the page organiser (select, rotate, delete, undo, real output preview);
drawing a signature into the real library, placing it and a date field, Settings validation and
clear-all-data; layout at 100/125/150/200 % scaling; accessibility-tree structure. Looking at the 200 %
screenshot found a real usability problem (page area squeezed to ~110 px with two side panels open),
fixed by hiding the thumbnail sidebar on narrow windows while Fill & Sign is open, now asserted by the test.

**Still not done:** anything behind native dialogs (Open/Save/folder pickers, Print) was not driven
interactively on Windows (Playwright cannot); opening output in **Acrobat Reader** (Chrome headless did not
render the PDFs, so no independent-viewer comparison exists); a real screen-reader pass (only the ARIA
structure/names were checked); installing the MSI (built, never installed); a heavy real-world 1,000-page
scan. See docs/MANUAL-QA.md.

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
Verified: the real `bulk_run` command driven through Tauri's mock runtime with real PDFium emits one
`bulk-progress` event per file then `bulk-finished` (statuses signed/skipped/signed, CSV report written,
right outputs on disk) — `bulk::tests::bulk_run_emits_progress_then_finished_events`. The event→UI
side is covered by the mocked Playwright flow. Not seen: the same thing in a real window (GTK dialog
automation was unreliable).

## 7. Hardening
Done: configurable limits, render/open timeouts, panic containment, fuzz harness (run, bug fixed), licence inventory
with a disallowed-licence gate, modal focus management (found by an E2E test), clippy clean in
shipped code, formatted. Not done: accessibility audit with a real screen reader; DPI testing; a
heavy real-world 1,000-page profile; installer polish; the MSI is per-machine.
