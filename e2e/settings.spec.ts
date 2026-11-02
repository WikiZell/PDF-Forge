import { expect, test } from "@playwright/test";
import { calls, installMock } from "./mock-backend";

test("settings: limits are validated by the backend, privacy text is shown, clearing data asks first", async ({ page }) => {
  await installMock(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Settings" }).click();
  const dlg = page.getByRole("dialog", { name: "Settings" });
  await expect(dlg).toBeVisible();
  await expect(dlg.getByText(/no network requests, sends no telemetry/)).toBeVisible();

  await dlg.getByLabel("Maximum file size (MB)").fill("0");
  await dlg.getByRole("button", { name: "Save limits" }).click();
  await expect(dlg.getByRole("alert")).toContainText("between 1 and 16384");
  await dlg.getByLabel("Maximum file size (MB)").fill("512");
  await dlg.getByRole("button", { name: "Save limits" }).click();
  await expect(dlg.getByRole("status")).toContainText("Saved");

  // declining the confirmation must not call the backend
  page.once("dialog", (d) => d.dismiss());
  await dlg.getByRole("button", { name: "Clear all app data…" }).click();
  expect((await calls(page)).some((c) => c.cmd === "clear_app_data")).toBe(false);
  page.once("dialog", (d) => d.accept());
  await dlg.getByRole("button", { name: "Clear all app data…" }).click();
  await expect(dlg.getByText("All app data was deleted.")).toBeVisible();
  expect((await calls(page)).some((c) => c.cmd === "clear_app_data")).toBe(true);
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
});
