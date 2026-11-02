import { expect, test } from "@playwright/test";
import { calls, installMock } from "./mock-backend";

test.describe("viewer", () => {
  test.beforeEach(async ({ page }) => {
    await installMock(page);
    await page.goto("/");
    await page.getByRole("button", { name: "Open…" }).click();
    await expect(page.getByRole("img", { name: "Page 1", exact: true })).toBeVisible();
  });

  test("opens a document, shows page count, thumbnails and outline", async ({ page }) => {
    await expect(page.getByText("/ 3")).toBeVisible();
    await expect(page.getByRole("button", { name: "Go to page 2" })).toBeVisible();
    await page.getByRole("tab", { name: "Bookmarks" }).click();
    await expect(page.getByRole("button", { name: "Section" })).toBeVisible();
    await page.getByRole("button", { name: "Section" }).click();
    await expect(page.getByLabel("Page number")).toHaveValue("3");
  });

  test("page navigation by buttons and keyboard; zoom steps; layouts", async ({ page }) => {
    await page.getByRole("button", { name: "Next page" }).click();
    await expect(page.getByLabel("Page number")).toHaveValue("2");
    await page.keyboard.press("End");
    await expect(page.getByLabel("Page number")).toHaveValue("3");
    await page.keyboard.press("Home");
    await expect(page.getByLabel("Page number")).toHaveValue("1");

    await page.getByRole("button", { name: "Zoom in" }).click();
    const pct = await page.getByLabel("Zoom percent").inputValue();
    expect(Number(pct)).toBeGreaterThan(0);
    await page.getByLabel("Zoom percent").fill("150");
    await expect(page.getByLabel("Zoom percent")).toHaveValue("150");
    await page.getByLabel("Zoom percent").fill("9999");
    await expect(page.getByLabel("Zoom percent")).toHaveValue("400");

    await page.getByLabel("Page layout").selectOption("two-page");
    await expect(page.getByRole("img", { name: "Page 1", exact: true })).toBeVisible();
    await expect(page.getByRole("img", { name: "Page 2", exact: true })).toBeVisible();
    await page.getByLabel("Page layout").selectOption("single");
    await expect(page.getByRole("img", { name: "Page 2", exact: true })).toHaveCount(0);
  });

  test("search highlights and steps through matches with F3", async ({ page }) => {
    await page.keyboard.press("Control+f");
    await page.getByLabel("Search text").fill("hello");
    await page.keyboard.press("Enter");
    await expect(page.getByText("Match 1 of 2")).toBeVisible();
    await page.keyboard.press("F3");
    await expect(page.getByText("Match 2 of 2")).toBeVisible();
    await expect(page.getByLabel("Page number")).toHaveValue("3");
    await page.getByLabel("Search text").fill("nothing");
    await page.keyboard.press("Enter");
    await expect(page.getByText("No matches.")).toBeVisible();
  });

  test("selectable text layer is present for copy", async ({ page }) => {
    await expect(page.getByText("Hello World").first()).toBeAttached();
  });

  test("view position is saved per file without storing content", async ({ page }) => {
    await page.getByRole("button", { name: "Next page" }).click();
    await page.waitForTimeout(900);
    const c = await calls(page);
    const saved = c.filter((x) => x.cmd === "save_view_state").pop();
    expect(saved?.args.view).toMatchObject({ page: 1, layout: "continuous" });
    expect(JSON.stringify(saved?.args)).not.toMatch(/password/i);
  });
});

test("password prompt: wrong then right password; password is not kept", async ({ page }) => {
  await installMock(page, { password: "s3cret" });
  await page.goto("/");
  await page.getByRole("button", { name: "Open…" }).click();
  const dlg = page.getByRole("dialog", { name: "Password required" });
  await expect(dlg).toBeVisible();
  await dlg.getByLabel("Password").fill("nope");
  await dlg.getByRole("button", { name: "Open" }).click();
  await expect(dlg.getByText("The password is incorrect.")).toBeVisible();
  await dlg.getByLabel("Password").fill("s3cret");
  await dlg.getByRole("button", { name: "Open" }).click();
  await expect(page.getByText("/ 3")).toBeVisible();
  // the only place the password was ever sent is the open call
  const c = await calls(page);
  const withPw = c.filter((x) => JSON.stringify(x.args).includes("s3cret"));
  expect(withPw.map((x) => x.cmd)).toEqual(["open_pdf"]);
  expect(await page.evaluate(() => JSON.stringify({ ...localStorage, ...sessionStorage }))).not.toContain("s3cret");
});
