import { test } from "@playwright/test";
import { installMock } from "./mock-backend";

// Design review screenshots (not part of the regular suite: only runs with DESIGN_SHOTS=1).
test.skip(!process.env.DESIGN_SHOTS, "design screenshots only");
const OUT = process.env.DESIGN_SHOTS_DIR ?? "/tmp/shots";
for (const scheme of ["light", "dark"] as const) {
  test(`design ${scheme}`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await page.setViewportSize({ width: 1360, height: 820 });
    await installMock(page);
    await page.goto("/");
    await page.screenshot({ path: `${OUT}/${scheme}-1-welcome.png` });
    await page.getByRole("button", { name: "Open…" }).click();
    await page.getByRole("img", { name: "Page 1", exact: true }).waitFor();
    await page.waitForTimeout(400);
    await page.screenshot({ path: `${OUT}/${scheme}-2-viewer.png` });
    await page.getByRole("button", { name: "Fill & Sign" }).click();
    await page.waitForTimeout(300);
    await page.screenshot({ path: `${OUT}/${scheme}-3-sign.png` });
    await page.getByRole("button", { name: "Fill & Sign" }).click();
    await page.getByRole("button", { name: "Organize pages" }).click();
    await page.waitForTimeout(500);
    await page.screenshot({ path: `${OUT}/${scheme}-4-organize.png` });
  });
}
