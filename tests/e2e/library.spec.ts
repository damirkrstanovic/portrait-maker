import { expect, test } from "@playwright/test";

for (const viewport of [{ width: 1080, height: 720 }, { width: 760, height: 560 }]) {
test(`keeps the ${viewport.width}×${viewport.height} library preview usable and restores focus through native view`, async ({ page }) => {
  const fixtureCount = viewport.width === 760 ? 1 : 201;
  const portraitName = fixtureCount === 1 ? "Old ranger" : "Fixture portrait 0";
  await page.setViewportSize(viewport);
  await page.goto(`/?testCatalog=1&count=${fixtureCount}`);
  await page.getByRole("button", { name: "Open library" }).click();

  const card = page.getByRole("button", { name: `Preview ${portraitName}` });
  await card.click();
  const preview = page.getByRole("complementary", { name: "Portrait preview" });
  await expect(preview).toBeVisible();
  await expect(page.getByRole("button", { name: "Hide preview" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Close preview" })).toBeVisible();

  const widths = await page.locator(".catalog-layout").evaluate((element) => ({ client: element.clientWidth, scroll: element.scrollWidth }));
  expect(widths.scroll).toBeLessThanOrEqual(widths.client);
  const previewBox = await preview.boundingBox();
  expect(previewBox).not.toBeNull();
  expect(previewBox!.x).toBeGreaterThanOrEqual(0);
  expect(previewBox!.y).toBeGreaterThanOrEqual(0);
  expect(previewBox!.y + previewBox!.height).toBeLessThanOrEqual(viewport.height);
  const headerBox = await page.locator(".library-header").boundingBox();
  expect(headerBox).not.toBeNull();
  expect(headerBox!.y).toBeGreaterThanOrEqual(0);

  const nativeTrigger = preview.getByRole("button", { name: "View Large at native resolution" });
  await nativeTrigger.click();
  const dialog = page.getByRole("dialog", { name: "Large native resolution" });
  await expect(dialog).toBeVisible();
  await expect(page.getByRole("button", { name: "Close native resolution" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("button", { name: "Close native resolution" })).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  await expect(page.getByRole("button", { name: "Close native resolution" })).toBeFocused();
  await page.keyboard.press("ArrowRight");
  await expect(preview.getByRole("heading", { name: portraitName })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(preview).toBeVisible();
  await expect(nativeTrigger).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(preview).toBeHidden();
  await expect(card).toBeFocused();
});
}

test("keeps a long destination path and import action inside the 1080×760 viewport", async ({ page }) => {
  await page.setViewportSize({ width: 1080, height: 760 });
  await page.goto("/?testCatalog=1&testDestinations=long");
  await page.getByRole("button", { name: "Open library" }).click();
  await page.getByRole("button", { name: "Game destinations" }).click();
  const dialog = page.getByRole("dialog", { name: "Game destinations" });
  await expect(dialog).toBeVisible();
  const modalWidth = await dialog.evaluate((element) => ({ client: element.clientWidth, scroll: element.scrollWidth }));
  expect(modalWidth.scroll).toBeLessThanOrEqual(modalWidth.client);
  const importButton = dialog.getByRole("button", { name: /Import existing portraits from/ });
  await expect(importButton).toBeVisible();
  const buttonWidth = await importButton.evaluate((element) => ({ client: element.clientWidth, scroll: element.scrollWidth }));
  expect(buttonWidth.scroll).toBeLessThanOrEqual(buttonWidth.client);
  const box = await dialog.boundingBox();
  expect(box).not.toBeNull();
  expect(box!.y + box!.height).toBeLessThanOrEqual(760);
});
