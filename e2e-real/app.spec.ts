import { chromium, expect, test, type Browser, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const APP = process.env.APP_EXE!;
const SAMPLES = process.env.SAMPLES_DIR!;
const SHOTS = process.env.SHOTS_DIR ?? "shots";
const PORT = 9333;

let proc: ChildProcess;
let browser: Browser;
let page: Page;

async function waitForCdp() {
  for (let i = 0; i < 60; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${PORT}/json/version`);
      if (r.ok) return;
    } catch { /* not up yet */ }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error("WebView2 debugging port never came up");
}

test.beforeAll(async () => {
  mkdirSync(SHOTS, { recursive: true });
  proc = spawn(APP, [join(SAMPLES, "merged.pdf")], {
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
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
  const target = page.locator('[data-page="0"]').first();
  const tb = (await target.boundingBox())!;
  await page.mouse.click(tb.x + tb.width / 2, tb.y + Math.min(tb.height / 2, 200));
  await expect(page.getByText("1 item(s) placed.")).toBeVisible();
  await page.getByRole("button", { name: "Date", exact: true }).click();
  await page.mouse.click(tb.x + 80, tb.y + 80);
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
    await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1280 / scale, height: 800 / scale, deviceScaleFactor: scale, mobile: false });
    await page.waitForTimeout(600);
    const m = await page.evaluate(() => ({
      overflowX: document.documentElement.scrollWidth - window.innerWidth,
      toolbarRight: Math.max(...Array.from(document.querySelectorAll('[role="toolbar"] button')).map((b) => b.getBoundingClientRect().right)),
      w: window.innerWidth,
      dpr: window.devicePixelRatio,
    }));
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
