# Manual QA checklist (cannot be automated here)

Run on Windows 10 and 11 against the installer from CI. Record what you observe.

## Interoperability (the spec requires Acrobat Reader and Chrome)
_Already checked automatically: Adobe Acrobat Reader (annotation counts: 5 annotated vs 4 flattened) and Edge's PDF viewer (PDFium, like Chrome) renders the app's signed (annotated and flattened) and merged output; see docs/MILESTONES.md. Acrobat Reader itself has not been tried._
- [ ] Signed copy (annotations mode) opens in **Acrobat Reader**: stamp / ink / text appear at the
      placed position and rotation; they can be selected, moved and deleted.
- [ ] Same in **Chrome** and **Edge**. Flatten mode: nothing is selectable, appearance identical.
- [ ] Merged/reordered/extracted output: bookmarks and links jump to the right pages in Acrobat.
- [ ] Merged form PDFs: renamed fields (e.g. `name_2`) are fillable and independent.
- [ ] Encrypted source (password) → output opens without a password.

## High DPI (100 %, 125 %, 150 %, 200 %)
_Automated on the real Windows app via CDP device-metrics emulation (no overflow, toolbar not clipped, page area >= 260 px); still worth a look on a real scaled display._
- [ ] Text, thumbnails and the page render crisp (PDFium is rendered at devicePixelRatio).
- [ ] Toolbar wraps, nothing is clipped; dialogs fit the screen at 200 %.
- [ ] Drag-reorder and signature handles hit correctly (pointer coordinates are CSS px).

## Accessibility
- [ ] Everything reachable with Tab/Shift+Tab; visible focus ring; no keyboard traps; dialogs
      trap focus, Escape closes and returns focus (covered by E2E for Split).
- [ ] (NVDA scripted pass done in CI; a person should repeat it) Screen reader (Narrator/NVDA): toolbar buttons, tabs, page grid ("Page 3, rotated 90
      degrees", selected state), signature items, progress bar and status messages are announced.
- [ ] Windows **High Contrast** theme: controls remain visible (CSS `forced-colors` rules).
- [ ] Keyboard: Ctrl+O/F/P/±/0, PageUp/Down, Home/End, F3, F4; organiser Ctrl+A/Z/Y, Delete,
      Alt+←/→; sign mode Ctrl+Z/Y, Delete, arrows, Esc.

## Platform
- [ ] "Open with" from Explorer; double-click is **not** hijacked (not the default handler).
- [ ] Installer per-user (NSIS) needs no admin prompt; WebView2 bootstrapper works offline-first-run.
- [ ] No outbound traffic (e.g. run with the network disabled / watch with a firewall log).
- [ ] 1,000-page real-world scan (not text-only): scroll, search, thumbnails, memory in Task Manager.
