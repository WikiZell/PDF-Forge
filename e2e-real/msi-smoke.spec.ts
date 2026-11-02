import { chromium, expect, test, type Browser, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// Attaches to the app installed by the MSI (APP_EXE) and checks that it really loads a document,
// not just that a window appears. The debugging port is enabled by the CI step via the WebView2 policy key.
const APP = process.env.APP_EXE!;
const PDF = process.env.PDF_FILE!;
const SHOTS = process.env.SHOTS_DIR ?? "shots";
const PORT = 9444;

let proc: ChildProcess;
let browser: Browser;
let page: Page;

test.beforeAll(async () => {
  test.setTimeout(240_000);
  mkdirSync(SHOTS, { recursive: true });
  proc = spawn(APP, [PDF], { stdio: "ignore" });
  const t0 = Date.now();
  for (;;) {
    try {
      if ((await fetch(`http://127.0.0.1:${PORT}/json/version`)).ok) break;
    } catch { /* not up yet */ }
    if (Date.now() - t0 > 120_000) throw new Error("WebView2 debugging port never came up for the MSI-installed app");
    await new Promise((r) => setTimeout(r, 1000));
  }
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
  while (!page && Date.now() - t0 < 150_000) {
    page = browser.contexts().flatMap((c) => c.pages()).find((p) => /tauri|localhost/.test(p.url())) as Page;
    if (!page) await new Promise((r) => setTimeout(r, 500));
  }
  if (!page) throw new Error("app page not found");
});

test.afterAll(async () => {
  await browser?.close().catch(() => {});
  proc?.kill();
});

test("the MSI-installed app loads its UI and renders the document", async () => {
  const timings: Record<string, number> = {};
  const t0 = Date.now();
  await expect(page.getByRole("button", { name: "Open…" })).toBeVisible({ timeout: 120_000 });
  timings.uiVisibleMs = Date.now() - t0;
  await expect(page.getByText("/ 3")).toBeVisible({ timeout: 60_000 });
  const img = page.locator('[data-page="0"] img').first();
  await expect(img).toBeVisible({ timeout: 60_000 });
  expect(await img.evaluate((i: HTMLImageElement) => i.naturalWidth)).toBeGreaterThan(100);
  timings.documentRenderedMs = Date.now() - t0;
  await page.screenshot({ path: join(SHOTS, "msi-app-loaded.png") });
  writeFileSync(join(SHOTS, "msi-timings.json"), JSON.stringify(timings, null, 2));
  console.log("MSI TIMINGS", JSON.stringify(timings));
});
