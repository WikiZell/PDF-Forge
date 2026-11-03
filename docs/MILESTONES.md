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

**Native dialogs driven on Windows with real keystrokes (all pass, `e2e-real/`):** the real Windows Open
dialog opens a PDF; the real Save dialog writes a signed copy both as annotations and flattened; the
organiser's multi-file merge dialog adds a PDF (2 + 5 = 7 pages) and its Save dialog writes the result;
the real Print dialog opens ("Total: 5 sheets of paper", rendered pages in the preview) and the app stays
healthy after it is closed. Originals stay untouched; outputs are valid PDFs.

**Independent viewer check (Microsoft Edge's PDF viewer, headed, on the same runner):** the files the app
saved open and render correctly - annotation version (1 of 2 pages, Edge shows its annotation marker on
the stamp, i.e. they are real, editable annotations), flattened version (same drawing, no marker, baked
in), and the 7-page organised/merged file. Edge uses PDFium like Chrome, so this is a good proxy for
Chrome but is **not** Acrobat Reader.

**Bug found by looking at these real-run screenshots:** placed signatures stayed in memory when a different
PDF was opened and could be stamped onto the wrong file. Fixed (sign state resets when another file
opens) with a regression test.

**Adobe Acrobat Reader (real, installed on the runner via Chocolatey) opened the app's saved files.** The
signed-with-annotations copy shows in Acrobat's Comments panel as "Comments 5" (4 on page 1, 1 on page 2,
each a "Signature" item with its stamp image and ink stroke visible), i.e. Acrobat treats them as real,
editable annotations; the flattened copy shows "Comments 4" - exactly one fewer, the signature that was
baked into the page. (A "make Acrobat your default?" prompt covers part of each screenshot, so rendering
was judged from the visible parts and the annotation counts, not a clean full-page view.)

**MSI (`windows-msi` job):** installed silently with `msiexec /qn` (per-machine), the installed app was
attached to over CDP and asserted to load its UI and render a document (UI visible ~0.2 s after attach; a
blank window seen in the first run was a cold WebView2 start), then uninstalled and verified gone.

**Screen-reader surface:** the real Windows UI Automation tree (what Narrator/NVDA read) has a
`ToolBar "Viewer controls"` with named buttons, `Spinner "Page number"`, `ComboBox "Page layout"`, the
Pages/Bookmarks/Search `TabItem`s, a `Group "Document viewer"` and `Image "Page N"` items
(`real-uia-tree.txt` on the `windows-smoke` branch). This verifies names and roles, not the experience of
using an actual screen reader.

**Bulk signing driven through the real window with the native folder pickers (passes):** input folder and
output folder chosen in the Windows folder dialogs, preview ("Would sign 3, skip 1, fail 1" with a
rendered sample), run (progress bar, "3 signed, 1 skipped, 1 failed", report saved), outputs and CSV on
disk, originals byte-identical. Found and fixed: Windows' internal `\\?\` path prefix was shown to users.

**Heavy document (generated 1,000-page JPEG "scan", 138 MB) on the real Windows app:** opens in ~6 s, jump
to page 500 in ~0.5 s, five fast jumps in ~1.9 s, search across all pages ~0.1 s, organiser loads in
~0.3 s and builds the 999-page output in ~0.4 s; memory ~200 MB while viewing, ~475 MB after the organiser.
It is a synthetic scan, not a real-world one.

**Genuine scanned document (run 41534422260, commit 2373b19):** the runner downloaded a real public-domain
archive.org book scan (568 pages, 25.4 MB, image-only pages) and the installed app opened it: first page
rendered in 1.7 s, "/ 568" shown, thumbnails and page bitmaps correct (`real-genuine-scan.png`,
`real-genuine-scan-metrics.json` on `windows-smoke`). A search for "the" took ~3 s and reported nothing
(no text layer, no OCR - by design; OCR is not in the spec).

**Real screen reader (NVDA 20xx on the runner, `nvda-speech.txt`):** NVDA was started with debug logging and
the app was tabbed through; NVDA announced "Viewer controls tool bar", each toolbar button by name with its
shortcut ("Open…, Control+O"), toggle states ("Fill & Sign toggle button not pressed", "Fit width pressed"),
"Page number spin button", "Page layout combo box", "Sidebar complementary landmark", "Pages tab 1 of 3",
"Go to page 1 button current page". It also exposed a real defect: the zoom field was announced as
"invalid entry" because `step=5` made values like 181 or 217 invalid; fixed (`step=1`). This is NVDA
driven by a script, not a blind or low-vision person using the app.

**Acrobat re-run (run 41538016995, commit 3048736):** the UI Automation click on "No" did not dismiss the
"make Acrobat your default?" prompt (Acrobat's window is not reachable that way on the runner), so a clean
full-page Acrobat screenshot was **not** obtained. What Acrobat does show behind the dimmed prompt is
consistent across both runs: the stamp image and ink stroke on page 1 and "Comments 5" (annotated copy) vs
"Comments 4" (flattened copy). Clean full-page renderings of the same files exist from Edge's PDF viewer and
Chrome (both PDFium). I am not pursuing this further in CI.

**Acrobat, final re-runs (runs 41541590294 and 41544983123):** key presses dismissed the "make Acrobat your
default?" prompt, and opening a throwaway file first absorbed the first-run upsell dialog. Real Adobe Acrobat
Reader now shows **both** the annotated copy (`acrobat-app-signed-annot.png`) and the flattened copy
(`acrobat-app-signed-flat.png`) on the `windows-smoke` branch, at full width, with the checkerboard stamp
image on both pages, the ink stroke and the text box visible; only a taskbar-pin toast sits in the corner.
Acrobat's comment panel counts them as 5 (annotated) vs 4 (flattened).

**UI redesign (commit 1beddc6, run 41597578899):** new design system (light/dark tokens, icon set, two-tier
grouped toolbar, refined sidebar/welcome/organizer/dialogs). Evidence: 19 mocked-IPC E2E and 52 unit tests
pass; the real installed app on Windows passes the same 14 real-app tests (native dialogs, bulk signing,
DPI layouts, scan metrics), the MSI install/launch/uninstall passes, and the NVDA pass still announces every
control by name (the earlier zoom-field "invalid entry" announcement is gone). Screenshots on `windows-smoke`
(`real-sign.png`, `real-bulk-done.png`, `real-viewer.png`). Design-review screenshots can be regenerated with
`DESIGN_SHOTS=1 npx playwright test e2e/design-shots.spec.ts`. Not formally done: a usability study with
real users, and a contrast audit beyond the token palette.

**Still not done:** a person using a screen reader; signing the installers (unsigned; SmartScreen will warn -
needs an Authenticode certificate, the CI hook is in place); OCR for scans (not in the spec); macOS/Linux
builds (not wired by design).

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
