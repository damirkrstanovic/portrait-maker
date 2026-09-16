import { expect, test } from "@playwright/test";

test("moves a selected portrait through trash, restore, and confirmed purge", async ({ page }) => {
  await page.setViewportSize({ width: 1080, height: 720 });
  await page.goto("/?testCatalog=1&count=1");
  await page.getByRole("button", { name: "Open library" }).click();

  await page.getByRole("checkbox", { name: "Select Old ranger" }).check();
  await page.getByRole("button", { name: "Move 1 selected to trash" }).click();
  await page.getByRole("radio", { name: "Trash" }).check();
  const mark = page.getByRole("checkbox", { name: "Mark Old ranger for trash action" });
  await expect(mark).toBeVisible();
  await mark.check();
  await page.getByRole("button", { name: "Restore 1 marked portrait" }).click();
  await page.getByRole("radio", { name: "Active portraits" }).check();
  await expect(page.getByRole("button", { name: "Preview Old ranger" })).toBeVisible();

  await page.getByRole("checkbox", { name: "Select Old ranger" }).check();
  await page.getByRole("button", { name: "Move 1 selected to trash" }).click();
  await page.getByRole("radio", { name: "Trash" }).check();
  await mark.check();
  await page.getByRole("button", { name: "Purge 1 marked portrait" }).click();
  const dialog = page.getByRole("dialog", { name: "Permanently delete portraits" });
  await expect(dialog).toContainText("Permanently delete 1 portrait");
  await dialog.getByRole("button", { name: "Permanently delete 1 portrait" }).click();
  await expect(page.getByText("No portraits match these filters.")).toBeVisible();
});
