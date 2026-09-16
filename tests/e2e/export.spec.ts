import { expect, test } from "@playwright/test";

for (const viewport of [{width:1080,height:720},{width:760,height:560}]) {
  test(`export preview stays bounded, restores focus, and invalidates at ${viewport.width}`, async ({page}) => {
    await page.setViewportSize(viewport);
    await page.goto('/?testCatalog=1&count=1395&testDestinations=long');
    await page.getByRole('button',{name:'Open library'}).click();
    const trigger=page.getByRole('button',{name:'Export portraits',exact:true});
    await trigger.click();
    const dialog=page.getByRole('dialog',{name:'Export game-ready portraits'});
    await expect(dialog.getByRole('button',{name:'Close export'})).toBeFocused();
    await page.keyboard.press('Shift+Tab');
    await expect(dialog.getByRole('button',{name:'Cancel',exact:true})).toBeFocused();
    await dialog.getByLabel('Saved destination',{exact:true}).selectOption('long-destination');
    const savedWidth=await dialog.evaluate(el=>({client:el.clientWidth,scroll:el.scrollWidth}));
    expect(savedWidth.scroll).toBeLessThanOrEqual(savedWidth.client);
    const path='/tmp/'+ 'very-long-portrait-collection-path-'.repeat(12) +'/Portraits';
    await dialog.getByLabel('Export target',{exact:true}).fill(path);
    await dialog.getByRole('button',{name:'Preview export'}).click();
    await expect(dialog.getByText('1395 frozen portraits')).toBeVisible();
    expect(await page.evaluate(()=>window.sessionStorage.getItem('exportApplyCount'))).toBeNull();
    const dimensions=await dialog.evaluate(el=>({client:el.clientWidth,scroll:el.scrollWidth}));
    expect(dimensions.scroll).toBeLessThanOrEqual(dimensions.client);
    const box=await dialog.boundingBox(); expect(box!.y+box!.height).toBeLessThanOrEqual(viewport.height);
    await page.screenshot({path:`test-results/export-preview-${viewport.width}.png`});
    await dialog.getByLabel('Export scope').selectOption('selected');
    await expect(dialog.getByText('1395 frozen portraits')).toBeHidden();
    await dialog.getByRole('button',{name:'Preview export'}).click();
    await expect(dialog.getByRole('alert')).toContainText('Select at least one active portrait');
    await dialog.getByLabel('Export scope').selectOption('all');
    await dialog.getByLabel('Export mode').selectOption('replace');
    await expect(dialog.getByRole('button',{name:'Preview export'})).toBeDisabled();
    await dialog.getByLabel('Use this exact directory as a portrait collection').check();
    await dialog.getByRole('button',{name:'Preview export'}).click();
    await expect(dialog.getByRole('button',{name:'Confirm replacement'})).toBeVisible();
    expect(await page.evaluate(()=>window.sessionStorage.getItem('exportApplyCount'))).toBeNull();
    await dialog.getByLabel('Export target',{exact:true}).fill('/tmp/changed');
    await expect(dialog.getByRole('button',{name:'Confirm replacement'})).toBeHidden();
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    await expect(trigger).toBeFocused();
  });
}

test('replacement applies the exact preview only after validation',async({page})=>{
  await page.goto('/?testCatalog=1&count=1');await page.getByRole('button',{name:'Open library'}).click();await page.getByRole('button',{name:'Export portraits',exact:true}).click();
  const dialog=page.getByRole('dialog',{name:'Export game-ready portraits'});await dialog.getByLabel('Export target',{exact:true}).fill('/tmp/Portraits');await dialog.getByLabel('Export mode').selectOption('replace');await dialog.getByLabel('Use this exact directory as a portrait collection').check();await dialog.getByRole('button',{name:'Preview export'}).click();await dialog.getByRole('button',{name:'Confirm replacement'}).click();await expect(dialog).toBeHidden();expect(await page.evaluate(()=>window.sessionStorage.getItem('exportApplyCount'))).toBe('1');expect(await page.evaluate(()=>window.sessionStorage.getItem('exportValidatedId'))).toBe('browser-export-plan');
});

for (const width of [1080,760]) {
 test(`export job report fits and restores focus at ${width}`,async({page})=>{
  await page.setViewportSize({width,height:width===760?560:720});
  await page.goto('/?testCatalog=1&count=1');await page.getByRole('button',{name:'Open library'}).click();const trigger=page.getByRole('button',{name:'Export portraits',exact:true});await trigger.click();
  const preview=page.getByRole('dialog',{name:'Export game-ready portraits'});await preview.getByLabel('Export target',{exact:true}).fill('/tmp/disposable-export');await preview.getByRole('button',{name:'Preview export'}).click();await preview.getByRole('button',{name:'Export portraits',exact:true}).click();
  const report=page.getByRole('dialog',{name:'Export finished'});await expect(report).toBeVisible();await expect(report.getByText('Files: 3 added · 0 overwritten · 0 removed · 2 preserved')).toBeVisible();await expect(report.getByRole('button',{name:'Close',exact:true})).toBeFocused();await page.keyboard.press('Tab');await expect(report.getByRole('button',{name:'Close',exact:true})).toBeFocused();const size=await report.evaluate(el=>({scroll:el.scrollWidth,width:el.clientWidth}));expect(size.scroll).toBeLessThanOrEqual(size.width);await page.screenshot({path:`test-results/export-report-${width}.png`});await page.keyboard.press('Escape');await expect(report).toBeHidden();await expect(trigger).toBeFocused();
 });
}

test('running export keeps a reachable cancel control',async({page})=>{
 await page.goto('/?testCatalog=1&count=1&exportPause=1');await page.getByRole('button',{name:'Open library'}).click();await page.getByRole('button',{name:'Export portraits',exact:true}).click();const preview=page.getByRole('dialog',{name:'Export game-ready portraits'});await preview.getByLabel('Export target',{exact:true}).fill('/tmp/disposable-export');await preview.getByRole('button',{name:'Preview export'}).click();await preview.getByRole('button',{name:'Export portraits',exact:true}).click();const progress=page.getByRole('dialog',{name:'Export in progress'});await expect(progress.getByRole('button',{name:'Cancel export'})).toBeFocused();await progress.getByRole('button',{name:'Cancel export'}).click();await expect(page.getByRole('dialog',{name:'Export finished'})).toContainText('Export cancelled');
});
