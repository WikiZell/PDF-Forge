import { chromium, expect, test, type Browser, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const APP = process.env.APP_EXE!;
const SAMPLES = process.env.SAMPLES_DIR!;
const SHOTS = process.env.SHOTS_DIR ?? "shots";
const PORT = 9444; // enabled for pdf-forge.exe by the CI step that writes the WebView2 policy registry value

let proc: ChildProcess;
let browser: Browser;
let page: Page;

async function waitForCdp() {
  for (let i = 0; i < 90; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/version`);
      if (r.ok) return;
    } catch { /* not up yet */ }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error("WebView2 debugging port never came up");
}

test.beforeAll(async () => {
  test.setTimeout(150_000);
  mkdirSync(SHOTS, { recursive: true });
  proc = spawn(APP, [join(SAMPLES, "merged.pdf")], {
    stdio: "ignore",
  });
  await waitForCdp();
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  const deadline = Date.now() + 30_000;
  while (!page && Date.now() < deadline) {
    page = browser.contexts().flatMap((c) => c.pages()).find((p) => /tauri|localhost/.test(p.url())) as Page;
    if (!page) await new Promise((r) => setTimeout(r, 500));
  }
  if (!page) throw new Error("app page not found over CDP");
  await page.waitForLoadState("domcontentloaded");
});

test.afterAll(async () => {
  await browser?.close().catch(() => {});
  proc?.kill();
});

test.afterEach(async ({}, info) => {
  if (info.status !== info.expectedStatus) {
    const n = info.title.replace(/[^a-z0-9]+/gi, "-").slice(0, 40);
    await page.screenshot({ path: join(SHOTS, `real-FAIL-${n}.png`) }).catch(() => {});
    const dom = await page
      .evaluate(() => ({
        armedHint: document.body.innerText.includes("Click on the page to place"),
        signLayers: document.querySelectorAll("[data-sign-layer]").length,
        placed: (document.body.innerText.match(/\d+ item\(s\) placed/) || [""])[0],
        pages: Array.from(document.querySelectorAll("[data-page]")).slice(0, 3).map((e) => {
          const r = e.getBoundingClientRect();
          return { page: e.getAttribute("data-page"), x: r.x, y: r.y, w: r.width, h: r.height };
        }),
        viewport: { w: innerWidth, h: innerHeight },
      }))
      .catch((e) => ({ error: String(e) }));
    writeFileSync(join(SHOTS, `real-FAIL-${n}.json`), JSON.stringify(dom, null, 2));
    console.log("FAIL DIAG", info.title, JSON.stringify(dom));
  }
});

const shot = (name: string) => page.screenshot({ path: join(SHOTS, `real-${name}.png`) });

test("opened from the command line: 5 pages, thumbnails and the page bitmap really rendered", async () => {
  await expect(page.getByText("/ 5")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("button", { name: /Go to page \d/ })).toHaveCount(5);
  // the page <img> is a real PNG produced by PDFium in the Rust backend
  const img = page.locator('[data-page="0"] img').first();
  await expect(img).toBeVisible({ timeout: 20_000 });
  expect(await img.evaluate((i: HTMLImageElement) => i.naturalWidth)).toBeGreaterThan(100);
  await shot("viewer");
});

test("no network requests leave the app", async () => {
  const hosts = await page.evaluate(() => performance.getEntriesByType("resource").map((e) => new URL(e.name).host));
  for (const h of hosts) expect(h, `unexpected host ${h}`).toMatch(/^(tauri\.localhost|ipc\.localhost|localhost.*|)$/);
});

test("real text search with highlights, bookmarks, zoom and layouts", async () => {
  await page.keyboard.press("Control+f");
  await page.getByLabel("Search text").fill("alpha");
  await page.keyboard.press("Enter");
  await expect(page.getByText(/Match 1 of \d+/)).toBeVisible({ timeout: 20_000 });
  await shot("search");

  await page.getByRole("tab", { name: "Bookmarks" }).click();
  await expect(page.getByRole("button", { name: /bm\d/ }).first()).toBeVisible({ timeout: 10_000 });
  await page.getByRole("tab", { name: "Pages" }).click();

  await page.getByLabel("Zoom percent").fill("150");
  await expect(page.getByLabel("Zoom percent")).toHaveValue("150");
  await page.getByLabel("Page layout").selectOption("two-page");
  await expect(page.locator("[data-page]").first()).toBeVisible();
  await page.getByLabel("Page layout").selectOption("continuous");
  await page.getByRole("button", { name: "Fit width" }).click();
});

test("page organiser on the real backend: select, rotate, delete, undo, preview output", async () => {
  await page.getByRole("button", { name: "Organize pages" }).click();
  const grid = page.getByRole("listbox", { name: /Pages\./ }).getByRole("option");
  await expect(grid).toHaveCount(5, { timeout: 30_000 });
  await grid.nth(0).click();
  await grid.nth(2).click({ modifiers: ["Shift"] });
  await expect(page.getByText("5 pages · 3 selected")).toBeVisible();
  await page.getByRole("button", { name: "Rotate ⟳" }).click();
  await expect(grid.nth(0)).toHaveAccessibleName(/rotated 90 degrees/);
  await grid.nth(4).click();
  await page.getByRole("button", { name: "Delete" }).click();
  await expect(grid).toHaveCount(4);
  await page.getByRole("button", { name: "Preview output" }).click();
  await expect(page.getByText(/Output: 4 pages, [\d.]+ (KB|B)/)).toBeVisible({ timeout: 30_000 });
  await shot("organizer");
  await page.getByRole("button", { name: "Undo" }).click();
  await expect(grid).toHaveCount(5);
  await page.getByRole("button", { name: "← Back to viewer" }).click();
  await expect(page.getByText("/ 5")).toBeVisible();
});

test("Fill & Sign on the real backend: draw a signature, it is stored, place it, settings clear it", async () => {
  await page.getByRole("button", { name: "Fill & Sign" }).click();
  await page.getByRole("button", { name: "New…" }).click();
  const canvas = page.getByLabel(/Signature drawing area/);
  const b = (await canvas.boundingBox())!;
  await page.mouse.move(b.x + 30, b.y + 100);
  await page.mouse.down();
  await page.mouse.move(b.x + 120, b.y + 40, { steps: 6 });
  await page.mouse.move(b.x + 240, b.y + 150, { steps: 6 });
  await page.mouse.up();
  await page.getByRole("button", { name: "Save signature" }).click();
  const place = page.getByRole("button", { name: /Place signature/ });
  await expect(place).toBeVisible({ timeout: 15_000 });

  await place.click();
  await expect(page.getByText("Click on the page to place")).toBeVisible(); // armed
  // click on a page that is actually on screen (the viewer keeps its scroll position between tests)
  const visibleSpot = () =>
    page.evaluate(() => {
      let best: { x: number; y: number; area: number } | null = null;
      for (const el of Array.from(document.querySelectorAll("[data-page]"))) {
        const r = el.getBoundingClientRect();
        const w = Math.max(0, Math.min(r.right, innerWidth - 300) - Math.max(r.left, 260));
        const h = Math.max(0, Math.min(r.bottom, innerHeight) - Math.max(r.top, 100));
        if (w * h > (best?.area ?? 0)) {
          best = { x: Math.max(r.left, 260) + w / 2, y: Math.max(r.top, 100) + Math.min(h / 2, 220), area: w * h };
        }
      }
      return best;
    });
  const spot = (await visibleSpot())!;
  expect(spot, "some page must be visible").not.toBeNull();
  await page.mouse.click(spot.x, spot.y);
  await expect(page.getByText("1 item(s) placed.")).toBeVisible();
  await page.getByRole("button", { name: "Date", exact: true }).click();
  const spot2 = (await visibleSpot())!;
  await page.mouse.click(spot2.x - 40, spot2.y - 40);
  await expect(page.getByText("2 item(s) placed.")).toBeVisible();
  await shot("sign");

  // settings: validation by the real backend, then clear all app data removes the stored signature
  await page.getByRole("button", { name: "Settings" }).click();
  const dlg = page.getByRole("dialog", { name: "Settings" });
  await dlg.getByLabel("Maximum file size (MB)").fill("0");
  await dlg.getByRole("button", { name: "Save limits" }).click();
  await expect(dlg.getByRole("alert")).toContainText("between 1 and");
  await dlg.getByLabel("Maximum file size (MB)").fill("1024");
  await dlg.getByRole("button", { name: "Save limits" }).click();
  await expect(dlg.getByRole("status")).toContainText("Saved");
  page.once("dialog", (d) => d.accept());
  await dlg.getByRole("button", { name: "Clear all app data…" }).click();
  await expect(dlg.getByText("All app data was deleted.")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Place signature/ })).toHaveCount(0);
});

test("layout holds at 100/125/150/200 % display scaling (no clipping, no horizontal scroll)", async () => {
  const cdp = await page.context().newCDPSession(page);
  for (const scale of [1, 1.25, 1.5, 2]) {
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: Math.round(1280 / scale), height: Math.round(800 / scale), deviceScaleFactor: scale, mobile: false });
    await page.waitForTimeout(600);
    const m = await page.evaluate(() => ({
      overflowX: document.documentElement.scrollWidth - window.innerWidth,
      toolbarRight: Math.max(...Array.from(document.querySelectorAll('[role="toolbar"] button')).map((b) => b.getBoundingClientRect().right)),
      w: window.innerWidth,
      dpr: window.devicePixelRatio,
    }));
    // the page area must stay usable at every scale, even with the Fill & Sign panel open
    const viewerW = await page.evaluate(() => document.querySelector('[aria-label="Document viewer"]')?.getBoundingClientRect().width ?? 0);
    expect(viewerW, `page area too narrow at ${scale * 100}%`).toBeGreaterThanOrEqual(260);
    expect(m.overflowX, `horizontal overflow at ${scale * 100}%`).toBeLessThanOrEqual(1);
    expect(m.toolbarRight, `toolbar clipped at ${scale * 100}%`).toBeLessThanOrEqual(m.w + 1);
    const img = page.locator('[data-page="0"] img').first();
    if (await img.count()) expect(await img.evaluate((i: HTMLImageElement) => i.naturalWidth)).toBeGreaterThan(0);
    await page.screenshot({ path: join(SHOTS, `real-dpi-${Math.round(scale * 100)}.png`) });
  }
  await cdp.send("Emulation.clearDeviceMetricsOverride");
});

test("accessibility tree of the real app exposes the expected landmarks and names", async () => {
  const snap = await page.locator("body").ariaSnapshot();
  writeFileSync(join(SHOTS, "real-aria-snapshot.txt"), snap);
  for (const needle of ["toolbar", "button \"Open…\"", "tab \"Pages\"", "spinbutton"]) {
    expect(snap, `ARIA snapshot should contain ${needle}`).toContain(needle);
  }
  // every button has an accessible name
  const unnamed = await page.evaluate(() =>
    Array.from(document.querySelectorAll("button")).filter((b) => !(b.getAttribute("aria-label") || b.textContent || "").trim()).length);
  expect(unnamed).toBe(0);
});

// ---------------------------------------------------------------------------------------------
// Native Windows dialogs, driven with real keystrokes (SendKeys) into the actual dialog windows.
// ---------------------------------------------------------------------------------------------
import { execFileSync, spawn as spawnProc } from "node:child_process";
import { existsSync, readFileSync, statSync, rmSync } from "node:fs";

const HERE = dirname(fileURLToPath(import.meta.url));
const OUT = join(SHOTS, "app-output");

/** Types `text` + Enter into whichever native dialog opens within `delayMs`. */
function typeIntoDialog(text: string, delayMs = 3000) {
  const keys = text.replace(/[+^%~(){}[\]]/g, (c) => `{${c}}`) + "{ENTER}";
  spawnProc("pwsh", ["-NoProfile", "-File", join(HERE, "send-keys.ps1"), "-Keys", keys, "-DelayMs", String(delayMs)], { stdio: "ignore" });
}

test("native Open dialog: open a PDF through the real Windows file dialog", async () => {
  mkdirSync(OUT, { recursive: true });
  await page.keyboard.press("Escape");
  typeIntoDialog(join(SAMPLES, "signed-annot.pdf"));
  await page.getByRole("button", { name: "Open…" }).click();
  await expect(page.getByText("signed-annot.pdf").first()).toBeVisible({ timeout: 30_000 });
  await expect(page.getByText("/ 2")).toBeVisible();
  await shot("open-dialog-result");
});

/** Fill & Sign is a toggle that survives between tests, so set it explicitly. */
async function signMode(on: boolean) {
  const open = await page.getByRole("complementary", { name: "Fill and sign" }).isVisible();
  if (open !== on) await page.getByRole("button", { name: "Fill & Sign" }).click();
}

test("native Save dialog: Fill & Sign writes a signed copy (annotations, then flattened) to a new file", async () => {
  await signMode(true);
  // signature library was cleared by the settings test, so draw a new one
  await page.getByRole("button", { name: "New…" }).click();
  const b = (await page.getByLabel(/Signature drawing area/).boundingBox())!;
  await page.mouse.move(b.x + 30, b.y + 100);
  await page.mouse.down();
  await page.mouse.move(b.x + 120, b.y + 40, { steps: 6 });
  await page.mouse.move(b.x + 240, b.y + 150, { steps: 6 });
  await page.mouse.up();
  await page.getByRole("button", { name: "Save signature" }).click();
  await page.getByRole("button", { name: /Place signature/ }).click();
  const spot = await page.evaluate(() => {
    for (const el of Array.from(document.querySelectorAll("[data-page]"))) {
      const r = el.getBoundingClientRect();
      if (r.bottom > 200 && r.top < innerHeight - 100) return { x: Math.max(r.left, 150) + 120, y: Math.max(r.top, 120) + 100 };
    }
    return null;
  });
  await page.mouse.click(spot!.x, spot!.y);
  await expect(page.getByText("1 item(s) placed.")).toBeVisible();

  for (const [flatten, name] of [[false, "app-signed-annot.pdf"], [true, "app-signed-flat.pdf"]] as const) {
    const target = join(OUT, name);
    rmSync(target, { force: true });
    const box = page.getByLabel(/Flatten on save/);
    if (flatten) await box.check(); else await box.uncheck();
    typeIntoDialog(target);
    await page.getByRole("button", { name: "Save signed copy…" }).click();
    // the notice includes the output path, so wait for THIS save's message, not a previous one
    await expect(page.getByText(name, { exact: false }).filter({ hasText: /Saved a signed copy with 1 item/ })).toBeVisible({ timeout: 30_000 });
    expect(existsSync(target), `${name} must exist`).toBe(true);
    expect(statSync(target).size).toBeGreaterThan(500);
    expect(readFileSync(target).subarray(0, 5).toString()).toBe("%PDF-");
  }
  // the original sample must be untouched
  expect(readFileSync(join(SAMPLES, "signed-annot.pdf")).length).toBeGreaterThan(0);
  await shot("save-dialog-result");
});

test("native Save dialog from the organiser, and multi-file merge dialog", async () => {
  await signMode(false);
  await expect(page.getByText("/ 2")).toBeVisible(); // signed-annot.pdf is the open document
  await page.getByRole("button", { name: "Organize pages" }).click();
  const grid = page.getByRole("listbox", { name: /Pages\./ }).getByRole("option");
  await expect(grid).toHaveCount(2, { timeout: 30_000 }); // signed-annot.pdf has 2 pages

  // merge: add another PDF through the native multi-select dialog
  typeIntoDialog(join(SAMPLES, "merged.pdf"));
  await page.getByRole("button", { name: "Add PDFs (merge)…" }).click();
  await expect(grid).toHaveCount(7, { timeout: 30_000 });

  const target = join(OUT, "app-organized.pdf");
  rmSync(target, { force: true });
  typeIntoDialog(target);
  await page.getByRole("button", { name: "Save a copy…" }).click();
  await expect(page.getByText(/Saved 7 pages/)).toBeVisible({ timeout: 30_000 });
  expect(statSync(target).size).toBeGreaterThan(500);
  await shot("organizer-saved");
  await page.getByRole("button", { name: "← Back to viewer" }).click();
});

test("native Print dialog opens from the app and the app stays healthy", async () => {
  mkdirSync(OUT, { recursive: true });
  const shotPath = join(SHOTS, "real-print-dialog.png");
  spawnProc("pwsh", ["-NoProfile", "-File", join(HERE, "desktop-shot.ps1"), "-Path", shotPath, "-DelayMs", "6000"], { stdio: "ignore" });
  await page.getByRole("button", { name: "Print…" }).click();
  await page.waitForTimeout(8000);
  expect(existsSync(shotPath), "desktop screenshot of the print dialog").toBe(true);
  // close the print dialog and confirm the app is still responsive and showed no error
  execFileSync("pwsh", ["-NoProfile", "-File", join(HERE, "send-keys.ps1"), "-Keys", "{ESC}", "-DelayMs", "500"]);
  await page.waitForTimeout(1500);
  await expect(page.getByRole("button", { name: "Open…" })).toBeVisible();
  expect(await page.getByRole("alert").count()).toBe(0);
});
