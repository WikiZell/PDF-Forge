import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { calls, installMock } from "./mock-backend";

// the blank-page size <select> also contains role=option elements, so scope to the page grid
const grid = (page: Page) => page.getByRole("listbox", { name: /Pages\./ }).getByRole("option");

test.beforeEach(async ({ page }) => {
  await installMock(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Open…" }).click();
  await page.getByRole("button", { name: "Organize pages" }).click();
  await expect(grid(page)).toHaveCount(3);
});

test("multi-select with click, shift and ctrl; select all", async ({ page }) => {
  const opts = grid(page);
  await opts.nth(0).click();
  await expect(opts.nth(0)).toHaveAttribute("aria-selected", "true");
  await opts.nth(2).click({ modifiers: ["Shift"] });
  await expect(page.getByText("3 pages · 3 selected")).toBeVisible();
  await opts.nth(1).click({ modifiers: ["Control"] });
  await expect(page.getByText("3 pages · 2 selected")).toBeVisible();
  await page.getByRole("listbox").press("Control+a");
  await expect(page.getByText("3 pages · 3 selected")).toBeVisible();
});

test("rotate, duplicate, delete and undo/redo through the toolbar and keyboard", async ({ page }) => {
  const opts = grid(page);
  await opts.nth(0).click();
  await page.getByRole("button", { name: "Rotate ⟳" }).click();
  await expect(opts.nth(0)).toHaveAccessibleName(/rotated 90 degrees/);
  await page.getByRole("button", { name: "Duplicate" }).click();
  await expect(opts).toHaveCount(4);
  await opts.nth(1).click();
  await page.getByRole("listbox").press("Delete");
  await expect(opts).toHaveCount(3);
  await page.getByRole("listbox").press("Control+z");
  await expect(opts).toHaveCount(4);
  await page.getByRole("listbox").press("Control+y");
  await expect(opts).toHaveCount(3);
  await page.getByRole("button", { name: "Undo" }).click();
  await page.getByRole("button", { name: "Undo" }).click();
  await page.getByRole("button", { name: "Undo" }).click();
  await expect(opts).toHaveCount(3);
  await expect(opts.nth(0)).not.toHaveAccessibleName(/rotated/);
  await expect(page.getByRole("button", { name: "Undo" })).toBeDisabled();
});

test("cannot delete every page", async ({ page }) => {
  await page.getByRole("listbox").press("Control+a");
  await expect(page.getByRole("button", { name: "Delete" })).toBeDisabled();
});

test("reorder with the keyboard (Alt+Arrow) and with the mouse", async ({ page }) => {
  const opts = grid(page);
  await opts.nth(0).click();
  await page.getByRole("listbox").press("Alt+ArrowRight");
  let c = await calls(page);
  expect(c.filter((x) => x.cmd === "org_edit").pop()?.args.edit).toMatchObject({ op: "move", indices: [0], to: 1 });

  // pointer drag: first page to after the last
  const a = await opts.nth(1).boundingBox();
  const last = await opts.nth(2).boundingBox();
  await page.mouse.move(a!.x + a!.width / 2, a!.y + a!.height / 2);
  await page.mouse.down();
  await page.mouse.move(last!.x + last!.width - 4, last!.y + last!.height / 2, { steps: 8 });
  await page.mouse.up();
  c = await calls(page);
  const mv = c.filter((x) => x.cmd === "org_edit").pop()?.args.edit;
  expect(mv).toMatchObject({ op: "move", to: 2 });
});

test("add blank page, preview shows renamed form fields", async ({ page }) => {
  await page.getByRole("button", { name: "Add blank after" }).click();
  await expect(grid(page)).toHaveCount(4);
  await page.getByRole("button", { name: "Preview output" }).click();
  await expect(page.getByText("Output: 4 pages, 2.0 KB")).toBeVisible();
  await expect(page.getByText('"name" → "name_2"')).toBeVisible();
});

test("organizer controls are keyboard reachable and labelled", async ({ page }) => {
  await expect(page.getByRole("toolbar", { name: "Page organizer tools" })).toBeVisible();
  await expect(page.getByRole("listbox", { name: /Pages\./ })).toBeVisible();
  await page.getByRole("button", { name: "Split…" }).click();
  const dlg = page.getByRole("dialog", { name: "Split document" });
  await expect(dlg).toBeVisible();
  // focus moves into the dialog, Tab stays inside it, Escape closes and returns focus to the opener
  expect(await page.evaluate(() => !!document.activeElement?.closest('[role="dialog"]'))).toBe(true);
  for (let i = 0; i < 12; i++) {
    await page.keyboard.press("Tab");
    expect(await page.evaluate(() => !!document.activeElement?.closest('[role="dialog"]'))).toBe(true);
  }
  await page.keyboard.press("Escape");
  await expect(dlg).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Split…" })).toBeFocused();
});
