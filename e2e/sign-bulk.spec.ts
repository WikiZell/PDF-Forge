import { expect, test } from "@playwright/test";
import { calls, installMock } from "./mock-backend";

async function drawSignature(page: import("@playwright/test").Page) {
  await page.getByRole("button", { name: "New…" }).click();
  const canvas = page.getByLabel(/Signature drawing area/);
  const b = (await canvas.boundingBox())!;
  await page.mouse.move(b.x + 30, b.y + 100);
  await page.mouse.down();
  await page.mouse.move(b.x + 120, b.y + 40, { steps: 5 });
  await page.mouse.move(b.x + 220, b.y + 140, { steps: 5 });
  await page.mouse.up();
  await page.getByRole("button", { name: "Save signature" }).click();
  await expect(page.getByRole("button", { name: "Place signature My signature" })).toBeVisible();
}

test.describe("fill & sign", () => {
  test.beforeEach(async ({ page }) => {
    await installMock(page);
    await page.goto("/");
    await page.getByRole("button", { name: "Open…" }).click();
    await page.getByRole("button", { name: "Fill & Sign" }).click();
    await expect(page.getByRole("complementary", { name: "Fill and sign" })).toBeVisible();
  });

  test("draw a signature, place it, move/resize/undo/delete", async ({ page }) => {
    await drawSignature(page);
    await page.getByRole("button", { name: "Place signature My signature" }).click();
    const target = page.getByRole("img", { name: "Page 1", exact: true });
    const tb = (await target.boundingBox())!;
    await page.mouse.click(tb.x + tb.width / 2, tb.y + tb.height / 2);
    const placed = page.getByRole("button", { name: /^Signature My signature/ });
    await expect(placed).toBeVisible();
    await expect(page.getByText("1 item(s) placed.")).toBeVisible();

    const before = (await placed.boundingBox())!;
    await page.mouse.move(before.x + before.width / 2, before.y + before.height / 2);
    await page.mouse.down();
    await page.mouse.move(before.x + before.width / 2 + 40, before.y + before.height / 2 + 30, { steps: 5 });
    await page.mouse.up();
    const moved = (await placed.boundingBox())!;
    expect(Math.round(moved.x - before.x)).toBeGreaterThanOrEqual(35);

    await page.getByRole("button", { name: "Undo" }).click();
    const back = (await placed.boundingBox())!;
    expect(Math.abs(back.x - before.x)).toBeLessThan(2);
    await page.getByRole("button", { name: "Redo" }).click();

    await placed.focus();
    await page.keyboard.press("Delete");
    await expect(page.getByText("0 item(s) placed.")).toBeVisible();
    await page.keyboard.press("Control+z");
    await expect(page.getByText("1 item(s) placed.")).toBeVisible();
  });

  test("date and text fields; edit text; save sends placements with flatten flag", async ({ page }) => {
    await page.getByRole("button", { name: "Text", exact: true }).click();
    const tb = (await page.getByRole("img", { name: "Page 1", exact: true }).boundingBox())!;
    await page.mouse.click(tb.x + 60, tb.y + 60);
    await page.getByLabel("Text", { exact: true }).fill("Approved by me");
    await page.getByRole("button", { name: "Date", exact: true }).click();
    await page.mouse.click(tb.x + 60, tb.y + 200);
    await expect(page.getByText("2 item(s) placed.")).toBeVisible();

    await page.getByLabel(/Flatten on save/).check();
    await page.getByRole("button", { name: "Save signed copy…" }).click();
    await expect(page.getByText(/Saved a signed copy with 2 item/)).toBeVisible();
    const c = await calls(page);
    const sd = c.find((x) => x.cmd === "sign_document")!;
    expect(sd.args.flatten).toBe(true);
    expect(sd.args.path).toBe("/mock/a.pdf");
    const texts = sd.args.placements.map((p: any) => p.text);
    expect(texts).toContain("Approved by me");
    expect(sd.args.placements.every((p: any) => p.page === 0 && p.w > 0 && p.h > 0)).toBe(true);
  });

  test("Escape disarms a tool; save is disabled with nothing placed", async ({ page }) => {
    await expect(page.getByRole("button", { name: "Save signed copy…" })).toBeDisabled();
    await page.getByRole("button", { name: "Date", exact: true }).click();
    await expect(page.getByText("Click on the page to place")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByText("Click on the page to place")).toHaveCount(0);
  });
});

test.describe("bulk sign", () => {
  test("form validation, preview report and run", async ({ page }) => {
    await installMock(page);
    await page.goto("/");
    await page.getByRole("button", { name: "Bulk sign…" }).click();
    await expect(page.getByRole("button", { name: /Run on 0 file/ })).toBeDisabled();
    await page.getByRole("button", { name: "Add folder…" }).click();
    await expect(page.getByText("2 file(s) selected")).toBeVisible();
    await page.getByRole("button", { name: "Choose output folder…" }).click();
    await expect(page.getByText("/mock/out")).toBeVisible();
    // no signature in the (mock) library yet -> clear message and no run
    await expect(page.getByText("Choose a signature.")).toBeVisible();
    await expect(page.getByRole("button", { name: /Run on 2 file/ })).toBeDisabled();
  });

  test("preview lists files that would not be signed; run sends the rule", async ({ page }) => {
    await installMock(page, { library: true });
    await page.goto("/");
    await page.getByRole("button", { name: "Bulk sign…" }).click();
    await page.getByRole("button", { name: "Add folder…" }).click();
    await page.getByRole("button", { name: "Choose output folder…" }).click();

    await page.getByLabel("Find text").fill("   ");
    await expect(page.getByText("Enter the anchor text to search for.")).toBeVisible();
    await expect(page.getByRole("button", { name: /Run on 2 file/ })).toBeDisabled();
    await page.getByLabel("Find text").fill("Signature:");

    await page.getByRole("button", { name: "Preview rule" }).click();
    await expect(page.getByText("Would sign 1, skip 1, fail 0.")).toBeVisible();
    await expect(page.getByText(/c2\.pdf: anchor text "Signature:" not found/)).toBeVisible();
    await expect(page.getByAltText(/Preview of the rule applied/)).toBeVisible();

    await page.getByRole("button", { name: /Run on 2 file/ }).click();
    const c = await calls(page);
    const run = c.find((x) => x.cmd === "bulk_run")!;
    expect(run.args.files).toEqual(["/mock/c1.pdf", "/mock/c2.pdf"]);
    expect(run.args.workers).toBe(3);
    expect(run.args.rule).toMatchObject({ version: 1, signature: "abc0", pages: "last", position: { type: "anchor", text: "Signature:" } });
    expect(run.args.today.year).toBeGreaterThan(2000);
    await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();
  });
});
