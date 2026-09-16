import { afterEach, beforeEach, expect, test } from "vitest";

import { loadCatalogPreferences, saveCatalogPreferences } from "./preferences";

const stored = new Map<string, string>();

beforeEach(() => {
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => stored.set(key, value),
    },
  });
});

afterEach(() => stored.clear());

test("remembers preview visibility and the grid role locally", () => {
  saveCatalogPreferences({ previewVisible: false, gridRole: "small", gridZoom: 150 });

  expect(loadCatalogPreferences()).toEqual({ previewVisible: false, gridRole: "small", gridZoom: 150 });
});
