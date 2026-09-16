import { expect, test } from "@playwright/test";

test("starts dense, resizes thumbnails, and remembers zoom after reopening", async ({ page }) => {
  await page.goto("/?testCatalog=1&count=201");
  await page.getByRole("button", { name: "Open library" }).click();
  const zoom = page.getByRole("slider", { name: "Thumbnail zoom" });
  await expect(zoom).toHaveValue("100");
  const row = page.locator(".portrait-grid-row").first();
  await expect.poll(() => row.locator(".portrait-card").count()).toBeGreaterThanOrEqual(5);
  const initialWidth = (await row.locator(".portrait-card").first().boundingBox())!.width;
  await zoom.fill("200");
  await expect.poll(async () => (await row.locator(".portrait-card").first().boundingBox())!.width).toBeGreaterThan(initialWidth * 1.5);
  await page.reload();
  await page.getByRole("button", { name: "Open library" }).click();
  await expect(page.getByRole("slider", { name: "Thumbnail zoom" })).toHaveValue("200");
  await page.getByRole("slider", { name: "Thumbnail zoom" }).fill("75");
  await expect.poll(() => page.locator(".portrait-grid-row").first().locator(".portrait-card").count()).toBeGreaterThanOrEqual(6);
  await page.setViewportSize({ width: 760, height: 560 });
  const dimensions = await page.locator(".portrait-grid-viewport").evaluate((element) => ({ width: element.clientWidth, scroll: element.scrollWidth }));
  expect(dimensions.scroll).toBeLessThanOrEqual(dimensions.width);
});
