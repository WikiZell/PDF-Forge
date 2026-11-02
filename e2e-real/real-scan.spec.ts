import { chromium, expect, test } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// Opens a genuine (not generated) scanned PDF, downloaded by the CI runner, in the real installed app.
const APP = process.env.APP_EXE!;
const SCAN = process.env.REAL_SCAN!;
const SHOTS = process.env.SHOTS_DIR ?? "shots";
const PORT = 9444;

test("genuine scanned document opens, renders, searches", async () => {
  test.setTimeout(240_000);
  mkdirSync(SHOTS, { recursive: true });
  const t: Record<string, number | string> = { fileMB: Math.round(statSync(SCAN).size / 1e5) / 10 };
  const t0 = Date.now();
  const proc = spawn(APP, [SCAN], { stdio: "ignore" });
  try {
    for (let i = 0; i < 90; i++) {
      try { if ((await fetch(`http://127.0.0.1:${PORT}/json/version`)).ok) break; } catch { /* wait */ }
      await new Promise((r) => setTimeout(r, 1000));
    }
    const browser = await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
    let page = browser.contexts().flatMap((c) => c.pages()).find((p) => /tauri|localhost/.test(p.url()));
    for (let i = 0; !page && i < 60; i++) {
      await new Promise((r) => setTimeout(r, 500));
      page = browser.contexts().flatMap((c) => c.pages()).find((p) => /tauri|localhost/.test(p.url()));
    }
    if (!page) throw new Error("app page not found");
    await expect(page.locator('[data-page="0"] img').first()).toBeVisible({ timeout: 120_000 });
    t.firstPageMs = Date.now() - t0;
    t.pageCountText = (await page.getByText(/^\/ \d+$/).first().textContent()) ?? "";
    const nat = await page.locator('[data-page="0"] img').first().evaluate((i: HTMLImageElement) => i.naturalWidth);
    expect(nat).toBeGreaterThan(200);
    await page.screenshot({ path: join(SHOTS, "real-genuine-scan.png") });
    const s0 = Date.now();
    await page.keyboard.press("Control+f");
    await page.getByLabel("Search text").fill("the");
    await page.keyboard.press("Enter");
    await page.waitForTimeout(3000);
    t.searchMs = Date.now() - s0;
    t.searchResultText = (await page.locator("body").innerText()).match(/No matches\.|\d+ matches?/)?.[0] ?? "";
    await page.screenshot({ path: join(SHOTS, "real-genuine-scan-search.png") });
    await browser.close();
  } finally {
    writeFileSync(join(SHOTS, "real-genuine-scan-metrics.json"), JSON.stringify(t, null, 2));
    console.log("GENUINE SCAN", JSON.stringify(t));
    proc.kill();
  }
});
